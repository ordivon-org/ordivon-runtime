using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

internal static class OrdivonWindowsJobLauncher
{
    private const uint JobObjectExtendedLimitInformationClass = 9;
    private const uint JobObjectCpuRateControlInformationClass = 15;
    private const uint JobObjectLimitActiveProcess = 0x00000008;
    private const uint JobObjectLimitJobMemory = 0x00000200;
    private const uint JobObjectLimitKillOnJobClose = 0x00002000;
    private const uint JobObjectCpuRateControlEnable = 0x1;
    private const uint JobObjectCpuRateControlHardCap = 0x4;
    private const uint CreateSuspended = 0x00000004;
    private const uint CreateUnicodeEnvironment = 0x00000400;
    private const uint StartfUseStdHandles = 0x00000100;
    private const uint Infinite = 0xffffffff;
    private const int StdInputHandle = -10;
    private const int StdOutputHandle = -11;
    private const int StdErrorHandle = -12;
    private const int InternalFailureExit = 125;

    [StructLayout(LayoutKind.Sequential)]
    private struct JobObjectBasicLimitInformation
    {
        public long PerProcessUserTimeLimit;
        public long PerJobUserTimeLimit;
        public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize;
        public UIntPtr MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public UIntPtr Affinity;
        public uint PriorityClass;
        public uint SchedulingClass;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct IoCounters
    {
        public ulong ReadOperationCount;
        public ulong WriteOperationCount;
        public ulong OtherOperationCount;
        public ulong ReadTransferCount;
        public ulong WriteTransferCount;
        public ulong OtherTransferCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JobObjectExtendedLimitInformation
    {
        public JobObjectBasicLimitInformation BasicLimitInformation;
        public IoCounters IoInfo;
        public UIntPtr ProcessMemoryLimit;
        public UIntPtr JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed;
        public UIntPtr PeakJobMemoryUsed;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JobObjectCpuRateControlInformation
    {
        public uint ControlFlags;
        public uint CpuRate;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct StartupInfo
    {
        public uint cb;
        public string lpReserved;
        public string lpDesktop;
        public string lpTitle;
        public uint dwX;
        public uint dwY;
        public uint dwXSize;
        public uint dwYSize;
        public uint dwXCountChars;
        public uint dwYCountChars;
        public uint dwFillAttribute;
        public uint dwFlags;
        public ushort wShowWindow;
        public ushort cbReserved2;
        public IntPtr lpReserved2;
        public IntPtr hStdInput;
        public IntPtr hStdOutput;
        public IntPtr hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessInformation
    {
        public IntPtr hProcess;
        public IntPtr hThread;
        public uint dwProcessId;
        public uint dwThreadId;
    }

    private sealed class Options
    {
        public string Executable;
        public string WorkingDirectory;
        public bool InheritEnvironment = true;
        public readonly Dictionary<string, string> Environment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        public readonly List<string> TargetArguments = new List<string>();
        public ulong? MemoryMaxBytes;
        public uint? ActiveProcessLimit;
        public uint? CpuQuotaPercent;
        public bool Diagnostics;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateJobObject(IntPtr jobAttributes, string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetInformationJobObject(IntPtr job, uint infoClass, IntPtr info, uint infoLength);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool QueryInformationJobObject(IntPtr job, uint infoClass, IntPtr info, uint infoLength, out uint returnLength);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool IsProcessInJob(IntPtr process, IntPtr job, out bool result);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcessW(
        string applicationName,
        StringBuilder commandLine,
        IntPtr processAttributes,
        IntPtr threadAttributes,
        bool inheritHandles,
        uint creationFlags,
        IntPtr environment,
        string currentDirectory,
        ref StartupInfo startupInfo,
        out ProcessInformation processInformation);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint ResumeThread(IntPtr thread);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateProcess(IntPtr process, uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr GetStdHandle(int stdHandle);

    public static int Main(string[] args)
    {
        try
        {
            Options options = ParseOptions(args);
            return Run(options);
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("ordivon-windows-job-launcher: " + error.Message);
            return InternalFailureExit;
        }
    }

    private static Options ParseOptions(string[] args)
    {
        Options options = new Options();
        int index = 0;
        while (index < args.Length)
        {
            string current = args[index++];
            if (current == "--")
            {
                while (index < args.Length)
                {
                    options.TargetArguments.Add(args[index++]);
                }
                break;
            }
            if (current == "--help")
            {
                throw new InvalidOperationException("usage: --executable PATH [--cwd PATH] [--inherit-environment true|false] [--env NAME=VALUE] [--memory-max-bytes N] [--active-process-limit N] [--cpu-quota-percent N] [--diagnostics] [-- ARGS...]");
            }
            if (current == "--diagnostics")
            {
                options.Diagnostics = true;
                continue;
            }
            string value = RequireValue(args, ref index, current);
            if (current == "--executable")
            {
                options.Executable = value;
            }
            else if (current == "--cwd")
            {
                options.WorkingDirectory = value;
            }
            else if (current == "--inherit-environment")
            {
                bool parsed;
                if (!Boolean.TryParse(value, out parsed))
                {
                    throw new InvalidOperationException("--inherit-environment must be true or false");
                }
                options.InheritEnvironment = parsed;
            }
            else if (current == "--env")
            {
                int equals = value.IndexOf('=');
                if (equals <= 0)
                {
                    throw new InvalidOperationException("--env must be NAME=VALUE");
                }
                string name = value.Substring(0, equals);
                if (name.IndexOf('\0') >= 0 || name.IndexOf('=') >= 0)
                {
                    throw new InvalidOperationException("environment name is invalid");
                }
                options.Environment[name] = value.Substring(equals + 1);
            }
            else if (current == "--memory-max-bytes")
            {
                ulong parsed;
                if (!UInt64.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out parsed) || parsed == 0)
                {
                    throw new InvalidOperationException("--memory-max-bytes must be a positive integer");
                }
                options.MemoryMaxBytes = parsed;
            }
            else if (current == "--active-process-limit")
            {
                uint parsed;
                if (!UInt32.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out parsed) || parsed == 0)
                {
                    throw new InvalidOperationException("--active-process-limit must be a positive integer");
                }
                options.ActiveProcessLimit = parsed;
            }
            else if (current == "--cpu-quota-percent")
            {
                uint parsed;
                if (!UInt32.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out parsed) || parsed == 0)
                {
                    throw new InvalidOperationException("--cpu-quota-percent must be a positive integer");
                }
                options.CpuQuotaPercent = parsed;
            }
            else
            {
                throw new InvalidOperationException("unknown launcher option: " + current);
            }
        }

        if (String.IsNullOrWhiteSpace(options.Executable))
        {
            throw new InvalidOperationException("--executable is required");
        }
        options.Executable = Path.GetFullPath(options.Executable);
        if (!File.Exists(options.Executable))
        {
            throw new InvalidOperationException("target executable does not exist: " + options.Executable);
        }
        if (String.IsNullOrWhiteSpace(options.WorkingDirectory))
        {
            options.WorkingDirectory = Path.GetDirectoryName(options.Executable);
        }
        options.WorkingDirectory = Path.GetFullPath(options.WorkingDirectory);
        if (!Directory.Exists(options.WorkingDirectory))
        {
            throw new InvalidOperationException("working directory does not exist: " + options.WorkingDirectory);
        }
        return options;
    }

    private static string RequireValue(string[] args, ref int index, string option)
    {
        if (index >= args.Length)
        {
            throw new InvalidOperationException(option + " requires a value");
        }
        return args[index++];
    }

    private static int Run(Options options)
    {
        IntPtr job = IntPtr.Zero;
        IntPtr environment = IntPtr.Zero;
        ProcessInformation pi = new ProcessInformation();
        bool processCreated = false;
        bool resumed = false;
        try
        {
            job = CreateJobObject(IntPtr.Zero, null);
            if (job == IntPtr.Zero)
            {
                ThrowWin32("CreateJobObject");
            }

            ConfigureExtendedLimits(job, options);
            uint cpuRate = ConfigureCpuRate(job, options.CpuQuotaPercent);

            environment = BuildEnvironment(options);
            StartupInfo si = new StartupInfo();
            si.cb = (uint)Marshal.SizeOf(typeof(StartupInfo));
            si.dwFlags = StartfUseStdHandles;
            si.hStdInput = GetStdHandle(StdInputHandle);
            si.hStdOutput = GetStdHandle(StdOutputHandle);
            si.hStdError = GetStdHandle(StdErrorHandle);

            StringBuilder commandLine = new StringBuilder(BuildCommandLine(options.Executable, options.TargetArguments));
            uint creationFlags = CreateSuspended | CreateUnicodeEnvironment;
            if (!CreateProcessW(
                    options.Executable,
                    commandLine,
                    IntPtr.Zero,
                    IntPtr.Zero,
                    true,
                    creationFlags,
                    environment,
                    options.WorkingDirectory,
                    ref si,
                    out pi))
            {
                ThrowWin32("CreateProcessW");
            }
            processCreated = true;

            if (!AssignProcessToJobObject(job, pi.hProcess))
            {
                ThrowWin32("AssignProcessToJobObject");
            }
            bool inJob;
            if (!IsProcessInJob(pi.hProcess, job, out inJob))
            {
                ThrowWin32("IsProcessInJob");
            }
            if (!inJob)
            {
                throw new InvalidOperationException("target process is not owned by the expected Job Object");
            }

            if (options.Diagnostics)
            {
                Console.Error.WriteLine(
                    "ORDIVON_WINDOWS_JOB_READY pid={0} memoryMaxBytes={1} activeProcessLimit={2} cpuQuotaPercent={3} cpuRate={4} logicalProcessors={5}",
                    pi.dwProcessId,
                    NullableText(options.MemoryMaxBytes),
                    NullableText(options.ActiveProcessLimit),
                    NullableText(options.CpuQuotaPercent),
                    cpuRate,
                    Environment.ProcessorCount);
            }

            uint resume = ResumeThread(pi.hThread);
            if (resume == 0xffffffff)
            {
                ThrowWin32("ResumeThread");
            }
            resumed = true;

            uint wait = WaitForSingleObject(pi.hProcess, Infinite);
            if (wait != 0)
            {
                throw new InvalidOperationException("WaitForSingleObject returned " + wait.ToString(CultureInfo.InvariantCulture));
            }
            uint exitCode;
            if (!GetExitCodeProcess(pi.hProcess, out exitCode))
            {
                ThrowWin32("GetExitCodeProcess");
            }
            if (options.Diagnostics)
            {
                Console.Error.WriteLine("ORDIVON_WINDOWS_JOB_EXIT pid={0} exitCode={1}", pi.dwProcessId, exitCode);
            }
            return unchecked((int)exitCode);
        }
        catch
        {
            if (processCreated && !resumed && pi.hProcess != IntPtr.Zero)
            {
                TerminateProcess(pi.hProcess, InternalFailureExit);
            }
            throw;
        }
        finally
        {
            if (environment != IntPtr.Zero)
            {
                Marshal.FreeHGlobal(environment);
            }
            if (pi.hThread != IntPtr.Zero)
            {
                CloseHandle(pi.hThread);
            }
            if (pi.hProcess != IntPtr.Zero)
            {
                CloseHandle(pi.hProcess);
            }
            if (job != IntPtr.Zero)
            {
                CloseHandle(job);
            }
        }
    }

    private static void ConfigureExtendedLimits(IntPtr job, Options options)
    {
        JobObjectExtendedLimitInformation info = new JobObjectExtendedLimitInformation();
        info.BasicLimitInformation.LimitFlags = JobObjectLimitKillOnJobClose;
        if (options.ActiveProcessLimit.HasValue)
        {
            info.BasicLimitInformation.LimitFlags |= JobObjectLimitActiveProcess;
            info.BasicLimitInformation.ActiveProcessLimit = options.ActiveProcessLimit.Value;
        }
        if (options.MemoryMaxBytes.HasValue)
        {
            info.BasicLimitInformation.LimitFlags |= JobObjectLimitJobMemory;
            info.JobMemoryLimit = new UIntPtr(options.MemoryMaxBytes.Value);
        }

        int size = Marshal.SizeOf(typeof(JobObjectExtendedLimitInformation));
        IntPtr pointer = Marshal.AllocHGlobal(size);
        try
        {
            Marshal.StructureToPtr(info, pointer, false);
            if (!SetInformationJobObject(job, JobObjectExtendedLimitInformationClass, pointer, (uint)size))
            {
                ThrowWin32("SetInformationJobObject(extended limits)");
            }
        }
        finally
        {
            Marshal.FreeHGlobal(pointer);
        }

        JobObjectExtendedLimitInformation actual = QueryExtendedLimits(job);
        uint expectedFlags = info.BasicLimitInformation.LimitFlags;
        if ((actual.BasicLimitInformation.LimitFlags & expectedFlags) != expectedFlags)
        {
            throw new InvalidOperationException("Job Object limit flag readback did not preserve requested limits");
        }
        if (options.ActiveProcessLimit.HasValue && actual.BasicLimitInformation.ActiveProcessLimit != options.ActiveProcessLimit.Value)
        {
            throw new InvalidOperationException("active process limit readback mismatch");
        }
        if (options.MemoryMaxBytes.HasValue && actual.JobMemoryLimit.ToUInt64() != options.MemoryMaxBytes.Value)
        {
            throw new InvalidOperationException("job memory limit readback mismatch");
        }
    }

    private static JobObjectExtendedLimitInformation QueryExtendedLimits(IntPtr job)
    {
        int size = Marshal.SizeOf(typeof(JobObjectExtendedLimitInformation));
        IntPtr pointer = Marshal.AllocHGlobal(size);
        try
        {
            uint returned;
            if (!QueryInformationJobObject(job, JobObjectExtendedLimitInformationClass, pointer, (uint)size, out returned))
            {
                ThrowWin32("QueryInformationJobObject(extended limits)");
            }
            return (JobObjectExtendedLimitInformation)Marshal.PtrToStructure(pointer, typeof(JobObjectExtendedLimitInformation));
        }
        finally
        {
            Marshal.FreeHGlobal(pointer);
        }
    }

    private static uint ConfigureCpuRate(IntPtr job, uint? cpuQuotaPercent)
    {
        if (!cpuQuotaPercent.HasValue)
        {
            return 0;
        }
        uint rate = CpuQuotaToWindowsRate(cpuQuotaPercent.Value, (uint)Environment.ProcessorCount);
        JobObjectCpuRateControlInformation info = new JobObjectCpuRateControlInformation();
        info.ControlFlags = JobObjectCpuRateControlEnable | JobObjectCpuRateControlHardCap;
        info.CpuRate = rate;
        int size = Marshal.SizeOf(typeof(JobObjectCpuRateControlInformation));
        IntPtr pointer = Marshal.AllocHGlobal(size);
        try
        {
            Marshal.StructureToPtr(info, pointer, false);
            if (!SetInformationJobObject(job, JobObjectCpuRateControlInformationClass, pointer, (uint)size))
            {
                ThrowWin32("SetInformationJobObject(cpu rate)");
            }
        }
        finally
        {
            Marshal.FreeHGlobal(pointer);
        }

        IntPtr query = Marshal.AllocHGlobal(size);
        try
        {
            uint returned;
            if (!QueryInformationJobObject(job, JobObjectCpuRateControlInformationClass, query, (uint)size, out returned))
            {
                ThrowWin32("QueryInformationJobObject(cpu rate)");
            }
            JobObjectCpuRateControlInformation actual = (JobObjectCpuRateControlInformation)Marshal.PtrToStructure(query, typeof(JobObjectCpuRateControlInformation));
            if (actual.ControlFlags != info.ControlFlags || actual.CpuRate != info.CpuRate)
            {
                throw new InvalidOperationException("CPU rate readback mismatch");
            }
        }
        finally
        {
            Marshal.FreeHGlobal(query);
        }
        return rate;
    }

    private static uint CpuQuotaToWindowsRate(uint quotaPercent, uint logicalProcessors)
    {
        if (logicalProcessors == 0)
        {
            throw new InvalidOperationException("logical processor count is zero");
        }
        ulong rate = ((ulong)quotaPercent * 100UL) / (ulong)logicalProcessors;
        if (rate == 0)
        {
            rate = 1;
        }
        if (rate > 10000UL)
        {
            rate = 10000UL;
        }
        return (uint)rate;
    }

    private static IntPtr BuildEnvironment(Options options)
    {
        SortedDictionary<string, string> environment = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        if (options.InheritEnvironment)
        {
            foreach (DictionaryEntry entry in Environment.GetEnvironmentVariables())
            {
                string name = entry.Key as string;
                string value = entry.Value as string;
                if (name != null && value != null)
                {
                    environment[name] = value;
                }
            }
        }
        foreach (KeyValuePair<string, string> pair in options.Environment)
        {
            environment[pair.Key] = pair.Value;
        }

        StringBuilder block = new StringBuilder();
        foreach (KeyValuePair<string, string> pair in environment)
        {
            if (pair.Key.IndexOf('\0') >= 0 || pair.Value.IndexOf('\0') >= 0)
            {
                throw new InvalidOperationException("environment contains NUL");
            }
            block.Append(pair.Key);
            block.Append('=');
            block.Append(pair.Value);
            block.Append('\0');
        }
        block.Append('\0');
        return Marshal.StringToHGlobalUni(block.ToString());
    }

    private static string BuildCommandLine(string executable, IList<string> arguments)
    {
        StringBuilder commandLine = new StringBuilder();
        commandLine.Append(QuoteWindowsArgument(executable));
        for (int i = 0; i < arguments.Count; ++i)
        {
            commandLine.Append(' ');
            commandLine.Append(QuoteWindowsArgument(arguments[i]));
        }
        return commandLine.ToString();
    }

    private static string QuoteWindowsArgument(string value)
    {
        if (value == null)
        {
            value = String.Empty;
        }
        bool needsQuotes = value.Length == 0;
        for (int i = 0; i < value.Length && !needsQuotes; ++i)
        {
            char c = value[i];
            needsQuotes = Char.IsWhiteSpace(c) || c == '"';
        }
        if (!needsQuotes)
        {
            return value;
        }

        StringBuilder quoted = new StringBuilder();
        quoted.Append('"');
        int backslashes = 0;
        for (int i = 0; i < value.Length; ++i)
        {
            char c = value[i];
            if (c == '\\')
            {
                ++backslashes;
                continue;
            }
            if (c == '"')
            {
                quoted.Append('\\', backslashes * 2 + 1);
                quoted.Append('"');
                backslashes = 0;
                continue;
            }
            quoted.Append('\\', backslashes);
            backslashes = 0;
            quoted.Append(c);
        }
        quoted.Append('\\', backslashes * 2);
        quoted.Append('"');
        return quoted.ToString();
    }

    private static string NullableText(ulong? value)
    {
        return value.HasValue ? value.Value.ToString(CultureInfo.InvariantCulture) : "none";
    }

    private static string NullableText(uint? value)
    {
        return value.HasValue ? value.Value.ToString(CultureInfo.InvariantCulture) : "none";
    }

    private static void ThrowWin32(string operation)
    {
        int code = Marshal.GetLastWin32Error();
        throw new InvalidOperationException(operation + " failed with Win32 error " + code.ToString(CultureInfo.InvariantCulture));
    }
}
