using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Security.AccessControl;
using System.Security.Principal;
using Microsoft.Win32;
using System.Text;
using System.Threading;

internal static class OrdivonWindowsJobFixture
{
    public static int Main(string[] args)
    {
        if (args.Length == 0)
        {
            Console.Error.WriteLine("fixture mode required");
            return 64;
        }
        string mode = args[0];
        if (mode == "normal") return Normal(args);
        if (mode == "tree") return Tree(args);
        if (mode == "tree-child") return TreeChild(args);
        if (mode == "sleep") return Sleep(args);
        if (mode == "process-limit") return ProcessLimit(args);
        if (mode == "memory") return Memory(args);
        if (mode == "cpu") return Cpu(args);
        if (mode == "echo") return Echo(args);
        if (mode == "authority-probe") return AuthorityProbe(args);
        if (mode == "immutable-input") return ImmutableInput(args);
        Console.Error.WriteLine("unknown fixture mode: " + mode);
        return 64;
    }

    private static int Normal(string[] args)
    {
        string marker = args[1];
        int exitCode = Int32.Parse(args[2]);
        Console.WriteLine("W1_FIXTURE_STDOUT marker=" + marker);
        Console.Error.WriteLine("W1_FIXTURE_STDERR marker=" + marker);
        return exitCode;
    }

    private static int Tree(string[] args)
    {
        string marker = args[1];
        StartSelf("tree-child", marker);
        Console.WriteLine("W1_TREE_READY marker=" + marker + " pid=" + Process.GetCurrentProcess().Id);
        Thread.Sleep(300000);
        return 0;
    }

    private static int TreeChild(string[] args)
    {
        string marker = args[1];
        StartSelf("sleep", marker);
        Thread.Sleep(300000);
        return 0;
    }

    private static int Sleep(string[] args)
    {
        Thread.Sleep(300000);
        return 0;
    }

    private static Process StartSelf(string mode, string marker)
    {
        ProcessStartInfo info = new ProcessStartInfo();
        info.FileName = Process.GetCurrentProcess().MainModule.FileName;
        info.Arguments = Quote(mode) + " " + Quote(marker);
        info.UseShellExecute = false;
        return Process.Start(info);
    }

    private static int ProcessLimit(string[] args)
    {
        string marker = args[1];
        try
        {
            Process child = StartSelf("sleep", marker);
            Thread.Sleep(200);
            if (child != null && !child.HasExited)
            {
                child.Kill();
                Console.Error.WriteLine("W1_PROCESS_LIMIT_FAILED marker=" + marker + " childPid=" + child.Id);
                return 55;
            }
            Console.WriteLine("W1_PROCESS_LIMIT_BLOCKED marker=" + marker);
            return 0;
        }
        catch (Win32Exception)
        {
            Console.WriteLine("W1_PROCESS_LIMIT_BLOCKED marker=" + marker);
            return 0;
        }
        catch (InvalidOperationException)
        {
            Console.WriteLine("W1_PROCESS_LIMIT_BLOCKED marker=" + marker);
            return 0;
        }
    }

    private static int Memory(string[] args)
    {
        string marker = args[1];
        int mb = Int32.Parse(args[2]);
        Console.WriteLine("W1_MEM_START marker=" + marker + " requestMB=" + mb);
        try
        {
            byte[] data = new byte[mb * 1024 * 1024];
            for (int i = 0; i < data.Length; i += 4096) data[i] = 1;
            Console.WriteLine("W1_MEM_ALLOCATED marker=" + marker + " bytes=" + data.LongLength);
            GC.KeepAlive(data);
            return 0;
        }
        catch (OutOfMemoryException)
        {
            Console.Error.WriteLine("W1_MEM_BLOCKED marker=" + marker);
            return 42;
        }
    }

    private static int Cpu(string[] args)
    {
        string marker = args[1];
        int ms = Int32.Parse(args[2]);
        Process process = Process.GetCurrentProcess();
        TimeSpan startCpu = process.TotalProcessorTime;
        Stopwatch wall = Stopwatch.StartNew();
        ulong state = 1;
        while (wall.ElapsedMilliseconds < ms)
        {
            state = unchecked(state * 6364136223846793005UL + 1UL);
        }
        wall.Stop();
        process.Refresh();
        double cpuMs = (process.TotalProcessorTime - startCpu).TotalMilliseconds;
        Console.WriteLine(
            "W1_CPU_RESULT marker={0} wallMs={1} cpuMs={2:F1} logical={3} state={4}",
            marker,
            wall.ElapsedMilliseconds,
            cpuMs,
            Environment.ProcessorCount,
            state);
        return 0;
    }

    private static int Echo(string[] args)
    {
        Console.WriteLine("W1_ECHO_CWD_B64=" + B64(Directory.GetCurrentDirectory()));
        Console.WriteLine("W1_ECHO_ENV_B64=" + B64(Environment.GetEnvironmentVariable("W1_ENV") ?? "<null>"));
        Console.WriteLine("W1_ECHO_SYSTEMROOT_B64=" + B64(Environment.GetEnvironmentVariable("SystemRoot") ?? "<null>"));
        Console.WriteLine("W1_ECHO_PATH_B64=" + B64(Environment.GetEnvironmentVariable("Path") ?? "<null>"));
        Console.WriteLine("W1_ECHO_WSL_DISTRO_B64=" + B64(Environment.GetEnvironmentVariable("WSL_DISTRO_NAME") ?? "<null>"));
        Console.WriteLine("W1_ECHO_ARGC=" + (args.Length - 1));
        for (int i = 1; i < args.Length; ++i)
        {
            Console.WriteLine("W1_ECHO_ARG_" + (i - 1) + "_B64=" + B64(args[i]));
        }
        return 0;
    }

    private static int AuthorityProbe(string[] args)
    {
        string marker = args[1];
        string keyPath = "SOFTWARE\\OrdivonRuntimeRw3\\" + marker;
        try
        {
            using (RegistryKey key = Registry.LocalMachine.CreateSubKey(keyPath, true))
            {
                if (key == null) throw new InvalidOperationException("HKLM CreateSubKey returned null");
                key.SetValue("marker", marker, RegistryValueKind.String);
                string observed = key.GetValue("marker") as string;
                if (observed != marker) throw new InvalidOperationException("HKLM marker readback mismatch");
            }
            Registry.LocalMachine.DeleteSubKeyTree(keyPath, false);
            Console.WriteLine("W1_AUTHORITY_HKLM=allowed marker=" + marker);
            return 0;
        }
        catch (UnauthorizedAccessException error)
        {
            Console.WriteLine("W1_AUTHORITY_HKLM=denied marker=" + marker + " type=" + error.GetType().Name);
            return 0;
        }
        catch (System.Security.SecurityException error)
        {
            Console.WriteLine("W1_AUTHORITY_HKLM=denied marker=" + marker + " type=" + error.GetType().Name);
            return 0;
        }
    }

    private static int ImmutableInput(string[] args)
    {
        string root = Environment.GetEnvironmentVariable("ORDIVON_INPUT_ROOT");
        if (String.IsNullOrWhiteSpace(root) || args.Length != 2)
        {
            Console.Error.WriteLine("W1_IMMUTABLE_INPUT_INVALID");
            return 64;
        }
        string path = Path.Combine(root, args[1].Replace('/', Path.DirectorySeparatorChar));
        string before = File.ReadAllText(path).Trim();
        string write = Attempt("write", delegate { File.WriteAllText(path, "MUTATED"); });
        string create = Attempt("create", delegate
        {
            string candidate = Path.Combine(root, "limited-created.txt");
            File.WriteAllText(candidate, "CREATED");
            File.Delete(candidate);
        });
        string renameFile = Attempt("rename-file", delegate
        {
            string moved = path + ".limited-moved";
            File.Move(path, moved);
            File.Move(moved, path);
        });
        string grant = Attempt("grant", delegate
        {
            SecurityIdentifier user = WindowsIdentity.GetCurrent().User;
            FileSecurity security = File.GetAccessControl(path);
            security.AddAccessRule(new FileSystemAccessRule(
                user, FileSystemRights.FullControl, AccessControlType.Allow));
            File.SetAccessControl(path, security);
        });
        string renameRoot = Attempt("rename-root", delegate
        {
            string moved = root + ".limited-moved";
            Directory.Move(root, moved);
            Directory.Move(moved, root);
        });
        string container = Directory.GetParent(root).FullName;
        string createSibling = Attempt("create-sibling", delegate
        {
            string sibling = Path.Combine(container, "limited-sibling.txt");
            File.WriteAllText(sibling, "CREATED");
            File.Delete(sibling);
        });
        string renameContainer = Attempt("rename-container", delegate
        {
            string moved = container + ".limited-moved";
            Directory.Move(container, moved);
            Directory.Move(moved, container);
        });
        string after = File.ReadAllText(path).Trim();
        Console.WriteLine("W1_IMMUTABLE_INPUT_ROOT_B64=" + B64(root));
        Console.WriteLine("W1_IMMUTABLE_INPUT_BEFORE_B64=" + B64(before));
        Console.WriteLine("W1_IMMUTABLE_INPUT_WRITE=" + write);
        Console.WriteLine("W1_IMMUTABLE_INPUT_CREATE=" + create);
        Console.WriteLine("W1_IMMUTABLE_INPUT_RENAME_FILE=" + renameFile);
        Console.WriteLine("W1_IMMUTABLE_INPUT_GRANT=" + grant);
        Console.WriteLine("W1_IMMUTABLE_INPUT_RENAME_ROOT=" + renameRoot);
        Console.WriteLine("W1_IMMUTABLE_INPUT_CREATE_SIBLING=" + createSibling);
        Console.WriteLine("W1_IMMUTABLE_INPUT_RENAME_CONTAINER=" + renameContainer);
        Console.WriteLine("W1_IMMUTABLE_INPUT_AFTER_B64=" + B64(after));
        bool denied = write.StartsWith("denied:", StringComparison.Ordinal)
            && create.StartsWith("denied:", StringComparison.Ordinal)
            && renameFile.StartsWith("denied:", StringComparison.Ordinal)
            && grant.StartsWith("denied:", StringComparison.Ordinal)
            && renameRoot.StartsWith("denied:", StringComparison.Ordinal)
            && createSibling.StartsWith("denied:", StringComparison.Ordinal)
            && renameContainer.StartsWith("denied:", StringComparison.Ordinal);
        return denied && before == after ? 0 : 57;
    }

    private static string Attempt(string operation, Action action)
    {
        try
        {
            action();
            return "succeeded";
        }
        catch (Exception error)
        {
            return "denied:" + error.GetType().Name;
        }
    }

    private static string B64(string value)
    {
        return Convert.ToBase64String(Encoding.UTF8.GetBytes(value));
    }

    private static string Quote(string value)
    {
        return "\"" + value.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
    }
}
