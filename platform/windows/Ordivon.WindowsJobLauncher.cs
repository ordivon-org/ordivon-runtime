using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Threading;

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
    private const uint WaitTimeout = 258;
    private const uint HandleFlagInherit = 0x00000001;
    private const int ErrorBrokenPipe = 109;
    private const uint TimedOutExit = 124;
    private const uint CancelledExit = 125;
    private const int StdInputHandle = -10;
    private const int StdOutputHandle = -11;
    private const int StdErrorHandle = -12;
    private const int InternalFailureExit = 125;
    private const uint TokenQuery = 0x0008;
    private const uint TokenDuplicate = 0x0002;
    private const uint TokenAssignPrimary = 0x0001;
    private const uint TokenAdjustDefault = 0x0080;
    private const uint LuaToken = 0x00000004;
    private const uint SeGroupEnabled = 0x00000004;
    private const uint SeGroupUseForDenyOnly = 0x00000010;
    private const uint SeGroupIntegrity = 0x00000020;
    private const int TokenGroupsClass = 2;
    private const int TokenTypeClass = 8;
    private const int TokenElevationTypeClass = 18;
    private const int TokenElevationClass = 20;
    private const int TokenIntegrityLevelClass = 25;
    private const int TokenPrimary = 1;
    private const int MediumIntegrityRid = 8192;

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

    [StructLayout(LayoutKind.Sequential)]
    private struct SecurityAttributes
    {
        public uint nLength;
        public IntPtr lpSecurityDescriptor;
        [MarshalAs(UnmanagedType.Bool)]
        public bool bInheritHandle;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct FileTime
    {
        public uint dwLowDateTime;
        public uint dwHighDateTime;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct TokenElevation
    {
        public int TokenIsElevated;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SidAndAttributes
    {
        public IntPtr Sid;
        public uint Attributes;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct TokenMandatoryLabel
    {
        public SidAndAttributes Label;
    }

    private sealed class TokenEvidence
    {
        public string Selection;
        public string UserSid;
        public int TokenType;
        public int ElevationType;
        public bool IsElevated;
        public int IntegrityLevelRid;
        public bool IsRestricted;
        public uint AdministratorsGroupAttributes;
    }

    private sealed class CapturedOutputInfo
    {
        public string ArtifactId;
        public string FileName;
        public string Digest;
        public ulong RetainedBytes;
        public ulong DroppedBytes;
        public bool Truncated;
    }

    private sealed class CaptureWorker
    {
        public IntPtr ReadHandle;
        public string Path;
        public ulong Limit;
        public ulong RetainedBytes;
        public ulong DroppedBytes;
        public Exception Error;

        public void Run()
        {
            try
            {
                byte[] buffer = new byte[16 * 1024];
                using (FileStream output = new FileStream(
                    Path, FileMode.CreateNew, FileAccess.Write, FileShare.Read, 4096, FileOptions.WriteThrough))
                {
                    while (true)
                    {
                        uint observed;
                        bool ok = ReadFile(ReadHandle, buffer, (uint)buffer.Length, out observed, IntPtr.Zero);
                        if (!ok)
                        {
                            int code = Marshal.GetLastWin32Error();
                            if (code == ErrorBrokenPipe)
                            {
                                break;
                            }
                            throw new InvalidOperationException(
                                "ReadFile(output pipe) failed with Win32 error " +
                                code.ToString(CultureInfo.InvariantCulture));
                        }
                        if (observed == 0)
                        {
                            break;
                        }
                        ulong remaining = Limit > RetainedBytes ? Limit - RetainedBytes : 0;
                        int writeLength = (int)Math.Min((ulong)observed, remaining);
                        if (writeLength > 0)
                        {
                            output.Write(buffer, 0, writeLength);
                            output.Flush();
                            RetainedBytes += (ulong)writeLength;
                        }
                        DroppedBytes += (ulong)observed - (ulong)writeLength;
                    }
                    output.Flush(true);
                }
            }
            catch (Exception error)
            {
                Error = error;
            }
            finally
            {
                if (ReadHandle != IntPtr.Zero)
                {
                    CloseHandle(ReadHandle);
                    ReadHandle = IntPtr.Zero;
                }
            }
        }
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
        public string RuntimeBundle;
        public string RuntimeJobId;
        public string RuntimeAttemptId;
        public string RuntimeLaunchTokenDigest;
        public string JobName;
        public ulong? StdoutLimitBytes;
        public ulong? StderrLimitBytes;
        public ulong? TimeoutMs;
        public bool DescribeRuntimeContext;
        public readonly List<string> ContextEnvironmentNames = new List<string>();

        public bool RuntimeMode
        {
            get { return !String.IsNullOrWhiteSpace(RuntimeBundle); }
        }
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
    private static extern bool TerminateJobObject(IntPtr job, uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CreatePipe(
        out IntPtr readPipe,
        out IntPtr writePipe,
        ref SecurityAttributes pipeAttributes,
        uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool ReadFile(
        IntPtr handle,
        [Out] byte[] buffer,
        uint bytesToRead,
        out uint bytesRead,
        IntPtr overlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr GetStdHandle(int stdHandle);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentProcessId();

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetProcessTimes(
        IntPtr process,
        out FileTime creationTime,
        out FileTime exitTime,
        out FileTime kernelTime,
        out FileTime userTime);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool QueryFullProcessImageName(
        IntPtr process,
        uint flags,
        StringBuilder executableName,
        ref uint size);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool OpenProcessToken(IntPtr process, uint desiredAccess, out IntPtr token);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool CreateRestrictedToken(
        IntPtr existingToken,
        uint flags,
        uint disableSidCount,
        IntPtr sidsToDisable,
        uint deletePrivilegeCount,
        IntPtr privilegesToDelete,
        uint restrictedSidCount,
        IntPtr sidsToRestrict,
        out IntPtr newToken);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool GetTokenInformation(
        IntPtr token, int tokenInformationClass, IntPtr tokenInformation, uint tokenInformationLength, out uint returnLength);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool SetTokenInformation(
        IntPtr token, int tokenInformationClass, IntPtr tokenInformation, uint tokenInformationLength);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertStringSidToSid(string stringSid, out IntPtr sid);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool EqualSid(IntPtr sid1, IntPtr sid2);

    [DllImport("advapi32.dll")]
    private static extern uint GetLengthSid(IntPtr sid);

    [DllImport("advapi32.dll")]
    private static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);

    [DllImport("advapi32.dll")]
    private static extern IntPtr GetSidSubAuthority(IntPtr sid, uint subAuthority);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool IsTokenRestricted(IntPtr token);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcessAsUserW(
        IntPtr token,
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

    [DllImport("userenv.dll", SetLastError = true)]
    private static extern bool CreateEnvironmentBlock(out IntPtr environment, IntPtr token, bool inherit);

    [DllImport("userenv.dll")]
    private static extern bool DestroyEnvironmentBlock(IntPtr environment);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);

    public static int Main(string[] args)
    {
        try
        {
            Options options = ParseOptions(args);
            return options.DescribeRuntimeContext ? DescribeRuntimeContext(options) : Run(options);
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
                throw new InvalidOperationException("usage: --executable PATH [--cwd PATH] [--inherit-environment true|false] [--env NAME=VALUE] [--memory-max-bytes N] [--active-process-limit N] [--cpu-quota-percent N] [--runtime-bundle PATH --runtime-job-id ID --runtime-attempt-id ID --runtime-launch-token-digest DIGEST --job-name NAME --timeout-ms N --stdout-limit-bytes N --stderr-limit-bytes N] [--diagnostics] [-- ARGS...]");
            }
            if (current == "--describe-runtime-context")
            {
                options.DescribeRuntimeContext = true;
                continue;
            }
            if (current == "--diagnostics")
            {
                options.Diagnostics = true;
                continue;
            }
            string value = RequireValue(args, ref index, current);
            if (current == "--context-env")
            {
                if (String.IsNullOrWhiteSpace(value) || value.IndexOf('=') >= 0 || value.IndexOf('\0') >= 0)
                {
                    throw new InvalidOperationException("--context-env name is invalid");
                }
                options.ContextEnvironmentNames.Add(value);
            }
            else if (current == "--executable")
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
            else if (current == "--runtime-bundle")
            {
                options.RuntimeBundle = value;
            }
            else if (current == "--runtime-job-id")
            {
                options.RuntimeJobId = value;
            }
            else if (current == "--runtime-attempt-id")
            {
                options.RuntimeAttemptId = value;
            }
            else if (current == "--runtime-launch-token-digest")
            {
                options.RuntimeLaunchTokenDigest = value;
            }
            else if (current == "--job-name")
            {
                options.JobName = value;
            }
            else if (current == "--timeout-ms")
            {
                ulong parsed;
                if (!UInt64.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out parsed) || parsed == 0)
                {
                    throw new InvalidOperationException("--timeout-ms must be a positive integer");
                }
                options.TimeoutMs = parsed;
            }
            else if (current == "--stdout-limit-bytes")
            {
                ulong parsed;
                if (!UInt64.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out parsed) || parsed == 0)
                {
                    throw new InvalidOperationException("--stdout-limit-bytes must be a positive integer");
                }
                options.StdoutLimitBytes = parsed;
            }
            else if (current == "--stderr-limit-bytes")
            {
                ulong parsed;
                if (!UInt64.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out parsed) || parsed == 0)
                {
                    throw new InvalidOperationException("--stderr-limit-bytes must be a positive integer");
                }
                options.StderrLimitBytes = parsed;
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

        if (options.DescribeRuntimeContext)
        {
            if (options.RuntimeMode || !String.IsNullOrWhiteSpace(options.Executable) || options.TargetArguments.Count != 0)
            {
                throw new InvalidOperationException("runtime context description cannot be combined with process execution");
            }
            if (options.ContextEnvironmentNames.Count == 0)
            {
                throw new InvalidOperationException("runtime context description requires at least one --context-env");
            }
            return options;
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
        if (options.RuntimeMode)
        {
            options.RuntimeBundle = Path.GetFullPath(options.RuntimeBundle);
            if (!Directory.Exists(options.RuntimeBundle))
            {
                throw new InvalidOperationException("runtime bundle does not exist: " + options.RuntimeBundle);
            }
            if (String.IsNullOrWhiteSpace(options.RuntimeJobId)
                || String.IsNullOrWhiteSpace(options.RuntimeAttemptId)
                || String.IsNullOrWhiteSpace(options.RuntimeLaunchTokenDigest)
                || String.IsNullOrWhiteSpace(options.JobName)
                || !options.TimeoutMs.HasValue
                || !options.StdoutLimitBytes.HasValue
                || !options.StderrLimitBytes.HasValue)
            {
                throw new InvalidOperationException("runtime mode requires complete Runtime identity, Job name, and output bounds");
            }
            if (!IsSha256Digest(options.RuntimeLaunchTokenDigest))
            {
                throw new InvalidOperationException("runtime launch-token digest is invalid");
            }
        }
        else if (options.StdoutLimitBytes.HasValue || options.StderrLimitBytes.HasValue)
        {
            throw new InvalidOperationException("output bounds require --runtime-bundle");
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

    private static int DescribeRuntimeContext(Options options)
    {
        IntPtr token = IntPtr.Zero;
        IntPtr environment = IntPtr.Zero;
        try
        {
            TokenEvidence evidence;
            token = AcquireLimitedExecutionToken(out evidence);
            if (!CreateEnvironmentBlock(out environment, token, false))
            {
                ThrowWin32("CreateEnvironmentBlock");
            }
            Dictionary<string, string> native = ReadEnvironmentBlock(environment);
            SortedDictionary<string, string> selected = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (string name in options.ContextEnvironmentNames)
            {
                string value;
                if (native.TryGetValue(name, out value))
                {
                    selected[name] = value;
                }
            }
            StringBuilder environmentJson = new StringBuilder();
            environmentJson.Append('{');
            bool first = true;
            foreach (KeyValuePair<string, string> pair in selected)
            {
                if (!first) environmentJson.Append(',');
                first = false;
                environmentJson.Append(JsonString(pair.Key));
                environmentJson.Append(':');
                environmentJson.Append(JsonString(pair.Value));
            }
            environmentJson.Append('}');
            string json = "{" +
                "\"schemaVersion\":1," +
                TokenEvidenceJsonFields(evidence) + "," +
                "\"environment\":" + environmentJson.ToString() +
                "}";
            Console.Out.WriteLine(json);
            return 0;
        }
        finally
        {
            if (environment != IntPtr.Zero) DestroyEnvironmentBlock(environment);
            if (token != IntPtr.Zero) CloseHandle(token);
        }
    }

    private static IntPtr AcquireLimitedExecutionToken(out TokenEvidence evidence)
    {
        IntPtr current = IntPtr.Zero;
        IntPtr limited = IntPtr.Zero;
        uint access = TokenQuery | TokenDuplicate | TokenAssignPrimary | TokenAdjustDefault;
        if (!OpenProcessToken(GetCurrentProcess(), access, out current))
        {
            ThrowWin32("OpenProcessToken");
        }
        try
        {
            TokenEvidence currentEvidence = ReadTokenEvidence(current, "current_limited");
            if (currentEvidence.TokenType != TokenPrimary)
            {
                throw new InvalidOperationException("launcher process token is not primary");
            }
            bool administratorsEnabled = GroupIsEnabled(currentEvidence.AdministratorsGroupAttributes);
            if (!currentEvidence.IsElevated
                && currentEvidence.IntegrityLevelRid <= MediumIntegrityRid
                && !administratorsEnabled)
            {
                evidence = currentEvidence;
                IntPtr selected = current;
                current = IntPtr.Zero;
                return selected;
            }
            if (!CreateRestrictedToken(
                    current,
                    LuaToken,
                    0,
                    IntPtr.Zero,
                    0,
                    IntPtr.Zero,
                    0,
                    IntPtr.Zero,
                    out limited))
            {
                ThrowWin32("CreateRestrictedToken(LUA_TOKEN)");
            }
            if (TokenIntegrityLevelRid(limited) > MediumIntegrityRid)
            {
                SetMediumIntegrity(limited);
            }
            evidence = ReadTokenEvidence(limited, "lua_medium_filtered");
            ValidateLimitedExecutionToken(currentEvidence, evidence);
            IntPtr selectedLimited = limited;
            limited = IntPtr.Zero;
            return selectedLimited;
        }
        finally
        {
            if (limited != IntPtr.Zero) CloseHandle(limited);
            if (current != IntPtr.Zero) CloseHandle(current);
        }
    }

    private static void ValidateLimitedExecutionToken(TokenEvidence source, TokenEvidence selected)
    {
        if (selected.TokenType != TokenPrimary
            || selected.IsElevated
            || selected.IntegrityLevelRid > MediumIntegrityRid
            || GroupIsEnabled(selected.AdministratorsGroupAttributes)
            || selected.UserSid != source.UserSid)
        {
            throw new InvalidOperationException("derived Windows execution token is not limited");
        }
        if (selected.Selection == "lua_medium_filtered"
            && selected.AdministratorsGroupAttributes != UInt32.MaxValue
            && (selected.AdministratorsGroupAttributes & SeGroupUseForDenyOnly) == 0)
        {
            throw new InvalidOperationException("LUA execution token did not preserve deny-only Administrator semantics");
        }
    }

    private static bool GroupIsEnabled(uint attributes)
    {
        return attributes != UInt32.MaxValue
            && (attributes & SeGroupEnabled) != 0
            && (attributes & SeGroupUseForDenyOnly) == 0;
    }

    private static TokenEvidence ReadTokenEvidence(IntPtr token, string selection)
    {
        TokenElevation elevation = TokenInformation<TokenElevation>(token, TokenElevationClass);
        using (WindowsIdentity identity = new WindowsIdentity(token))
        {
            if (identity.User == null)
            {
                throw new InvalidOperationException("execution token has no user SID");
            }
            return new TokenEvidence {
                Selection = selection,
                UserSid = identity.User.Value,
                TokenType = TokenInformation<int>(token, TokenTypeClass),
                ElevationType = TokenInformation<int>(token, TokenElevationTypeClass),
                IsElevated = elevation.TokenIsElevated != 0,
                IntegrityLevelRid = TokenIntegrityLevelRid(token),
                IsRestricted = IsTokenRestricted(token),
                AdministratorsGroupAttributes = GroupAttributes(token, "S-1-5-32-544"),
            };
        }
    }

    private static T TokenInformation<T>(IntPtr token, int informationClass) where T : struct
    {
        uint required;
        GetTokenInformation(token, informationClass, IntPtr.Zero, 0, out required);
        if (required == 0)
        {
            ThrowWin32("GetTokenInformation(size)");
        }
        IntPtr buffer = Marshal.AllocHGlobal(checked((int)required));
        try
        {
            if (!GetTokenInformation(token, informationClass, buffer, required, out required))
            {
                ThrowWin32("GetTokenInformation");
            }
            return (T)Marshal.PtrToStructure(buffer, typeof(T));
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static int TokenIntegrityLevelRid(IntPtr token)
    {
        uint required;
        GetTokenInformation(token, TokenIntegrityLevelClass, IntPtr.Zero, 0, out required);
        if (required == 0)
        {
            ThrowWin32("GetTokenInformation(integrity size)");
        }
        IntPtr buffer = Marshal.AllocHGlobal(checked((int)required));
        try
        {
            if (!GetTokenInformation(token, TokenIntegrityLevelClass, buffer, required, out required))
            {
                ThrowWin32("GetTokenInformation(integrity)");
            }
            TokenMandatoryLabel label = (TokenMandatoryLabel)Marshal.PtrToStructure(
                buffer, typeof(TokenMandatoryLabel));
            IntPtr countPointer = GetSidSubAuthorityCount(label.Label.Sid);
            if (countPointer == IntPtr.Zero)
            {
                ThrowWin32("GetSidSubAuthorityCount");
            }
            byte count = Marshal.ReadByte(countPointer);
            if (count == 0)
            {
                throw new InvalidOperationException("integrity SID has no RID");
            }
            IntPtr ridPointer = GetSidSubAuthority(label.Label.Sid, (uint)(count - 1));
            if (ridPointer == IntPtr.Zero)
            {
                ThrowWin32("GetSidSubAuthority");
            }
            return Marshal.ReadInt32(ridPointer);
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static uint GroupAttributes(IntPtr token, string sidText)
    {
        IntPtr expectedSid;
        if (!ConvertStringSidToSid(sidText, out expectedSid))
        {
            ThrowWin32("ConvertStringSidToSid(group)");
        }
        try
        {
            uint required;
            GetTokenInformation(token, TokenGroupsClass, IntPtr.Zero, 0, out required);
            if (required == 0)
            {
                ThrowWin32("GetTokenInformation(groups size)");
            }
            IntPtr buffer = Marshal.AllocHGlobal(checked((int)required));
            try
            {
                if (!GetTokenInformation(token, TokenGroupsClass, buffer, required, out required))
                {
                    ThrowWin32("GetTokenInformation(groups)");
                }
                uint count = unchecked((uint)Marshal.ReadInt32(buffer));
                int offset = IntPtr.Size == 8 ? 8 : 4;
                int stride = Marshal.SizeOf(typeof(SidAndAttributes));
                for (uint index = 0; index < count; ++index)
                {
                    SidAndAttributes group = (SidAndAttributes)Marshal.PtrToStructure(
                        IntPtr.Add(buffer, offset + checked((int)index) * stride),
                        typeof(SidAndAttributes));
                    if (EqualSid(group.Sid, expectedSid))
                    {
                        return group.Attributes;
                    }
                }
                return UInt32.MaxValue;
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
        }
        finally
        {
            LocalFree(expectedSid);
        }
    }

    private static void SetMediumIntegrity(IntPtr token)
    {
        IntPtr sid;
        if (!ConvertStringSidToSid("S-1-16-8192", out sid))
        {
            ThrowWin32("ConvertStringSidToSid(medium integrity)");
        }
        try
        {
            TokenMandatoryLabel label = new TokenMandatoryLabel();
            label.Label.Sid = sid;
            label.Label.Attributes = SeGroupIntegrity;
            int structSize = Marshal.SizeOf(typeof(TokenMandatoryLabel));
            IntPtr buffer = Marshal.AllocHGlobal(structSize);
            try
            {
                Marshal.StructureToPtr(label, buffer, false);
                uint length = checked((uint)structSize + GetLengthSid(sid));
                if (!SetTokenInformation(token, TokenIntegrityLevelClass, buffer, length))
                {
                    ThrowWin32("SetTokenInformation(medium integrity)");
                }
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
        }
        finally
        {
            LocalFree(sid);
        }
    }

    private static Dictionary<string, string> ReadEnvironmentBlock(IntPtr environment)
    {
        Dictionary<string, string> values = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        long cursor = environment.ToInt64();
        while (true)
        {
            string entry = Marshal.PtrToStringUni(new IntPtr(cursor));
            if (String.IsNullOrEmpty(entry))
            {
                break;
            }
            cursor += checked((entry.Length + 1) * 2);
            int equals = entry.IndexOf('=');
            if (equals <= 0)
            {
                // Drive-current-directory pseudo variables (=C:=...) are irrelevant because
                // Runtime always supplies an absolute executable and explicit working directory.
                continue;
            }
            values[entry.Substring(0, equals)] = entry.Substring(equals + 1);
        }
        return values;
    }

    private static string TokenEvidenceJsonFields(TokenEvidence evidence)
    {
        return "\"tokenSelection\":" + JsonString(evidence.Selection) + "," +
            "\"tokenUserSid\":" + JsonString(evidence.UserSid) + "," +
            "\"tokenType\":" + evidence.TokenType.ToString(CultureInfo.InvariantCulture) + "," +
            "\"tokenElevationType\":" + evidence.ElevationType.ToString(CultureInfo.InvariantCulture) + "," +
            "\"tokenIsElevated\":" + (evidence.IsElevated ? "true" : "false") + "," +
            "\"tokenIntegrityLevelRid\":" + evidence.IntegrityLevelRid.ToString(CultureInfo.InvariantCulture) + "," +
            "\"tokenIsRestricted\":" + (evidence.IsRestricted ? "true" : "false") + "," +
            "\"administratorsGroupAttributes\":" + evidence.AdministratorsGroupAttributes.ToString(CultureInfo.InvariantCulture);
    }

    private static int Run(Options options)
    {
        return options.RuntimeMode ? RunRuntime(options) : RunLegacy(options);
    }

    private static int RunRuntime(Options options)
    {
        IntPtr job = IntPtr.Zero;
        IntPtr environment = IntPtr.Zero;
        IntPtr executionToken = IntPtr.Zero;
        TokenEvidence tokenEvidence = null;
        IntPtr stdoutWrite = IntPtr.Zero;
        IntPtr stderrWrite = IntPtr.Zero;
        CaptureWorker stdoutCapture = null;
        CaptureWorker stderrCapture = null;
        Thread stdoutThread = null;
        Thread stderrThread = null;
        ProcessInformation pi = new ProcessInformation();
        bool processCreated = false;
        bool resumed = false;
        long startedUnixMs = UnixTimeMilliseconds();
        bool timedOut = false;
        bool cancelled = false;
        try
        {
            job = CreateJobObject(IntPtr.Zero, options.JobName);
            if (job == IntPtr.Zero)
            {
                ThrowWin32("CreateJobObject");
            }
            ConfigureExtendedLimits(job, options);
            uint cpuRate = ConfigureCpuRate(job, options.CpuQuotaPercent);
            executionToken = AcquireLimitedExecutionToken(out tokenEvidence);
            environment = BuildEnvironment(options);

            stdoutCapture = CreateCaptureWorker(
                Path.Combine(options.RuntimeBundle, "stdout.log"),
                options.StdoutLimitBytes.Value,
                out stdoutWrite);
            stderrCapture = CreateCaptureWorker(
                Path.Combine(options.RuntimeBundle, "stderr.log"),
                options.StderrLimitBytes.Value,
                out stderrWrite);

            StartupInfo si = new StartupInfo();
            si.cb = (uint)Marshal.SizeOf(typeof(StartupInfo));
            si.dwFlags = StartfUseStdHandles;
            si.hStdInput = GetStdHandle(StdInputHandle);
            si.hStdOutput = stdoutWrite;
            si.hStdError = stderrWrite;

            StringBuilder commandLine = new StringBuilder(BuildCommandLine(options.Executable, options.TargetArguments));
            uint creationFlags = CreateSuspended | CreateUnicodeEnvironment;
            if (!CreateProcessAsUserW(
                    executionToken,
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
                ThrowWin32("CreateProcessAsUserW");
            }
            processCreated = true;
            CloseHandle(stdoutWrite);
            stdoutWrite = IntPtr.Zero;
            CloseHandle(stderrWrite);
            stderrWrite = IntPtr.Zero;

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

            WriteWindowsStartEvidence(options, pi, tokenEvidence);
            stdoutThread = new Thread(stdoutCapture.Run);
            stderrThread = new Thread(stderrCapture.Run);
            stdoutThread.IsBackground = true;
            stderrThread.IsBackground = true;
            stdoutThread.Start();
            stderrThread.Start();

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

            while (true)
            {
                uint wait = WaitForSingleObject(pi.hProcess, 20);
                if (wait == 0)
                {
                    break;
                }
                if (wait != WaitTimeout)
                {
                    throw new InvalidOperationException(
                        "WaitForSingleObject returned " + wait.ToString(CultureInfo.InvariantCulture));
                }
                if (File.Exists(Path.Combine(options.RuntimeBundle, "cancel-requested.json")))
                {
                    cancelled = true;
                    TerminateWholeJob(job, CancelledExit);
                    break;
                }
                long elapsed = UnixTimeMilliseconds() - startedUnixMs;
                if (elapsed >= 0 && (ulong)elapsed >= options.TimeoutMs.Value)
                {
                    timedOut = true;
                    TerminateWholeJob(job, TimedOutExit);
                    break;
                }
            }

            uint finalWait = WaitForSingleObject(pi.hProcess, 5000);
            if (finalWait != 0)
            {
                throw new InvalidOperationException(
                    "target did not reach terminal state after Job termination");
            }
            uint exitCode;
            if (!GetExitCodeProcess(pi.hProcess, out exitCode))
            {
                ThrowWin32("GetExitCodeProcess");
            }

            // Closing the final Job handle kills any descendants that outlived the direct child.
            // Only after that boundary can captured output be finalized without later mutation.
            CloseHandle(job);
            job = IntPtr.Zero;
            stdoutThread.Join();
            stderrThread.Join();
            if (stdoutCapture.Error != null)
            {
                throw new InvalidOperationException("stdout capture failed: " + stdoutCapture.Error.Message);
            }
            if (stderrCapture.Error != null)
            {
                throw new InvalidOperationException("stderr capture failed: " + stderrCapture.Error.Message);
            }

            CapturedOutputInfo stdout = CapturedOutputFromWorker(options, stdoutCapture, true);
            CapturedOutputInfo stderr = CapturedOutputFromWorker(options, stderrCapture, false);
            int signedExitCode = unchecked((int)exitCode);
            WriteRuntimeResult(
                options,
                signedExitCode,
                timedOut,
                cancelled,
                startedUnixMs,
                UnixTimeMilliseconds(),
                stdout,
                stderr);
            if (options.Diagnostics)
            {
                Console.Error.WriteLine(
                    "ORDIVON_WINDOWS_JOB_EXIT pid={0} exitCode={1} timedOut={2} cancelled={3}",
                    pi.dwProcessId, signedExitCode, timedOut, cancelled);
            }
            return signedExitCode;
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
            if (stdoutWrite != IntPtr.Zero) CloseHandle(stdoutWrite);
            if (stderrWrite != IntPtr.Zero) CloseHandle(stderrWrite);
            if (environment != IntPtr.Zero) Marshal.FreeHGlobal(environment);
            if (executionToken != IntPtr.Zero) CloseHandle(executionToken);
            if (pi.hThread != IntPtr.Zero) CloseHandle(pi.hThread);
            if (pi.hProcess != IntPtr.Zero) CloseHandle(pi.hProcess);
            if (job != IntPtr.Zero) CloseHandle(job);
        }
    }

    private static CaptureWorker CreateCaptureWorker(string path, ulong limit, out IntPtr writeHandle)
    {
        SecurityAttributes attributes = new SecurityAttributes();
        attributes.nLength = (uint)Marshal.SizeOf(typeof(SecurityAttributes));
        attributes.bInheritHandle = true;
        IntPtr readHandle;
        if (!CreatePipe(out readHandle, out writeHandle, ref attributes, 0))
        {
            ThrowWin32("CreatePipe(output)");
        }
        if (!SetHandleInformation(readHandle, HandleFlagInherit, 0))
        {
            int code = Marshal.GetLastWin32Error();
            CloseHandle(readHandle);
            CloseHandle(writeHandle);
            writeHandle = IntPtr.Zero;
            throw new InvalidOperationException(
                "SetHandleInformation(output) failed with Win32 error " +
                code.ToString(CultureInfo.InvariantCulture));
        }
        return new CaptureWorker { ReadHandle = readHandle, Path = path, Limit = limit };
    }

    private static void TerminateWholeJob(IntPtr job, uint exitCode)
    {
        if (!TerminateJobObject(job, exitCode))
        {
            ThrowWin32("TerminateJobObject");
        }
    }

    private static CapturedOutputInfo CapturedOutputFromWorker(
        Options options, CaptureWorker worker, bool stdout)
    {
        string fileName = stdout ? "stdout.log" : "stderr.log";
        string path = Path.Combine(options.RuntimeBundle, fileName);
        FileInfo info = new FileInfo(path);
        if (checked((ulong)info.Length) != worker.RetainedBytes)
        {
            throw new InvalidOperationException("captured output length differs from retained count");
        }
        return new CapturedOutputInfo {
            ArtifactId = options.RuntimeAttemptId + (stdout ? ".stdout" : ".stderr"),
            FileName = fileName,
            Digest = Sha256File(path),
            RetainedBytes = worker.RetainedBytes,
            DroppedBytes = worker.DroppedBytes,
            Truncated = worker.DroppedBytes != 0,
        };
    }

    private static int RunLegacy(Options options)
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

    private static void WriteWindowsStartEvidence(Options options, ProcessInformation pi, TokenEvidence tokenEvidence)
    {
        FileTime creation;
        FileTime exit;
        FileTime kernel;
        FileTime user;
        if (!GetProcessTimes(pi.hProcess, out creation, out exit, out kernel, out user))
        {
            ThrowWin32("GetProcessTimes");
        }
        ulong creationValue = ((ulong)creation.dwHighDateTime << 32) | creation.dwLowDateTime;
        string imagePath = QueryProcessImagePath(pi.hProcess);
        string imageDigest = Sha256File(options.Executable);
        string json = "{" +
            "\"schemaVersion\":1," +
            "\"jobId\":" + JsonString(options.RuntimeJobId) + "," +
            "\"attemptId\":" + JsonString(options.RuntimeAttemptId) + "," +
            "\"launchTokenDigest\":" + JsonString(options.RuntimeLaunchTokenDigest) + "," +
            "\"jobName\":" + JsonString(options.JobName) + "," +
            "\"launcherProcessId\":" + GetCurrentProcessId().ToString(CultureInfo.InvariantCulture) + "," +
            "\"processId\":" + pi.dwProcessId.ToString(CultureInfo.InvariantCulture) + "," +
            "\"processCreationTimeFileTime\":" + creationValue.ToString(CultureInfo.InvariantCulture) + "," +
            "\"imagePath\":" + JsonString(imagePath) + "," +
            "\"imageDigest\":" + JsonString(imageDigest) + "," +
            TokenEvidenceJsonFields(tokenEvidence) + "," +
            "\"observedUnixMs\":" + UnixTimeMilliseconds().ToString(CultureInfo.InvariantCulture) +
            "}";
        WriteTextAtomic(Path.Combine(options.RuntimeBundle, "windows-start.json"), json);
    }

    private static string QueryProcessImagePath(IntPtr process)
    {
        uint size = 32768;
        StringBuilder path = new StringBuilder((int)size);
        if (!QueryFullProcessImageName(process, 0, path, ref size))
        {
            ThrowWin32("QueryFullProcessImageName");
        }
        return path.ToString();
    }

    private static void WriteRuntimeResult(
        Options options,
        int exitCode,
        bool timedOut,
        bool cancelled,
        long startedUnixMs,
        long finishedUnixMs,
        CapturedOutputInfo stdout,
        CapturedOutputInfo stderr)
    {
        bool succeeded = !timedOut && !cancelled && exitCode == 0;
        string status = cancelled ? "CANCELLED" : (succeeded ? "COMPLETED" : "FAILED");
        string stepStatus = cancelled ? "cancelled" : (timedOut ? "timed_out" : (exitCode == 0 ? "succeeded" : "failed"));
        string step = "{" +
            "\"id\":\"command\"," +
            "\"index\":0," +
            "\"status\":" + JsonString(stepStatus) + "," +
            "\"exitCode\":" + exitCode.ToString(CultureInfo.InvariantCulture) + "," +
            "\"timedOut\":" + (timedOut ? "true" : "false") + "," +
            "\"continued\":false," +
            "\"startedUnixMs\":" + startedUnixMs.ToString(CultureInfo.InvariantCulture) + "," +
            "\"finishedUnixMs\":" + finishedUnixMs.ToString(CultureInfo.InvariantCulture) +
            "}";
        string failure = succeeded ? String.Empty :
            ",\"failedStepId\":\"command\",\"failedStepIndex\":0";
        string json = "{" +
            "\"schemaVersion\":1," +
            "\"taskId\":" + JsonString(options.RuntimeAttemptId) + "," +
            "\"jobId\":" + JsonString(options.RuntimeJobId) + "," +
            "\"attemptId\":" + JsonString(options.RuntimeAttemptId) + "," +
            "\"launchTokenDigest\":" + JsonString(options.RuntimeLaunchTokenDigest) + "," +
            "\"status\":" + JsonString(status) + "," +
            "\"exitCode\":" + exitCode.ToString(CultureInfo.InvariantCulture) + "," +
            "\"timedOut\":" + (timedOut ? "true" : "false") + "," +
            "\"startedUnixMs\":" + startedUnixMs.ToString(CultureInfo.InvariantCulture) + "," +
            "\"finishedUnixMs\":" + finishedUnixMs.ToString(CultureInfo.InvariantCulture) + "," +
            "\"steps\":[" + step + "]" + failure + "," +
            "\"stdout\":" + CapturedOutputJson(stdout) + "," +
            "\"stderr\":" + CapturedOutputJson(stderr) +
            "}";
        WriteTextAtomic(Path.Combine(options.RuntimeBundle, "result.json"), json);
    }

    private static string CapturedOutputJson(CapturedOutputInfo output)
    {
        return "{" +
            "\"artifactId\":" + JsonString(output.ArtifactId) + "," +
            "\"fileName\":" + JsonString(output.FileName) + "," +
            "\"digest\":" + JsonString(output.Digest) + "," +
            "\"retainedBytes\":" + output.RetainedBytes.ToString(CultureInfo.InvariantCulture) + "," +
            "\"droppedBytes\":" + output.DroppedBytes.ToString(CultureInfo.InvariantCulture) + "," +
            "\"truncated\":" + (output.Truncated ? "true" : "false") +
            "}";
    }

    private static void WriteTextAtomic(string path, string content)
    {
        string temporary = path + ".tmp-" + GetCurrentProcessId().ToString(CultureInfo.InvariantCulture) + "-" + Guid.NewGuid().ToString("N");
        byte[] bytes = new UTF8Encoding(false).GetBytes(content);
        using (FileStream stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
        {
            stream.Write(bytes, 0, bytes.Length);
            stream.Flush(true);
        }
        if (File.Exists(path))
        {
            File.Delete(temporary);
            throw new InvalidOperationException("runtime evidence path already exists: " + path);
        }
        File.Move(temporary, path);
    }

    private static string Sha256File(string path)
    {
        using (SHA256 sha = SHA256.Create())
        using (FileStream stream = File.OpenRead(path))
        {
            byte[] digest = sha.ComputeHash(stream);
            StringBuilder text = new StringBuilder("sha256:");
            for (int index = 0; index < digest.Length; ++index)
            {
                text.Append(digest[index].ToString("x2", CultureInfo.InvariantCulture));
            }
            return text.ToString();
        }
    }

    private static string JsonString(string value)
    {
        if (value == null)
        {
            return "null";
        }
        StringBuilder output = new StringBuilder();
        output.Append('"');
        foreach (char c in value)
        {
            switch (c)
            {
                case '"': output.Append("\\\""); break;
                case '\\': output.Append("\\\\"); break;
                case '\b': output.Append("\\b"); break;
                case '\f': output.Append("\\f"); break;
                case '\n': output.Append("\\n"); break;
                case '\r': output.Append("\\r"); break;
                case '\t': output.Append("\\t"); break;
                default:
                    if (c < 0x20)
                    {
                        output.Append("\\u");
                        output.Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                    }
                    else
                    {
                        output.Append(c);
                    }
                    break;
            }
        }
        output.Append('"');
        return output.ToString();
    }

    private static long UnixTimeMilliseconds()
    {
        return (long)(DateTime.UtcNow - new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc)).TotalMilliseconds;
    }

    private static bool IsSha256Digest(string value)
    {
        if (value == null || !value.StartsWith("sha256:", StringComparison.Ordinal) || value.Length != 71)
        {
            return false;
        }
        for (int index = 7; index < value.Length; ++index)
        {
            char c = value[index];
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')))
            {
                return false;
            }
        }
        return true;
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
