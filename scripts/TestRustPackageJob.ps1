if (-not ('LitdGate.JobNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace LitdGate {
  [StructLayout(LayoutKind.Sequential)]
  public struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    public long PerProcessUserTimeLimit; public long PerJobUserTimeLimit; public uint LimitFlags;
    public UIntPtr MinimumWorkingSetSize; public UIntPtr MaximumWorkingSetSize; public uint ActiveProcessLimit;
    public UIntPtr Affinity; public uint PriorityClass; public uint SchedulingClass;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct IO_COUNTERS {
    public ulong ReadOperationCount; public ulong WriteOperationCount; public ulong OtherOperationCount;
    public ulong ReadTransferCount; public ulong WriteTransferCount; public ulong OtherTransferCount;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation; public IO_COUNTERS IoInfo;
    public UIntPtr ProcessMemoryLimit; public UIntPtr JobMemoryLimit;
    public UIntPtr PeakProcessMemoryUsed; public UIntPtr PeakJobMemoryUsed;
  }
  public static class JobNative {
    public const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000;
    public const int JobObjectExtendedLimitInformation = 9;
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern IntPtr CreateJobObject(IntPtr a, string name);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool SetInformationJobObject(IntPtr hJob, int infoClass, ref JOBOBJECT_EXTENDED_LIMIT_INFORMATION info, uint len);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool AssignProcessToJobObject(IntPtr hJob, IntPtr hProcess);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool TerminateJobObject(IntPtr hJob, uint exitCode);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool CloseHandle(IntPtr h);
  }
}
'@
}

function New-KillOnCloseJob {
    $job = [LitdGate.JobNative]::CreateJobObject([IntPtr]::Zero, $null)
    if ($job -eq [IntPtr]::Zero) {
        throw "TEST_GATE_JOB_CREATE_FAILED: CreateJobObject win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $info = New-Object 'LitdGate.JOBOBJECT_EXTENDED_LIMIT_INFORMATION'
    $info.BasicLimitInformation.LimitFlags = [LitdGate.JobNative]::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    $length = [Runtime.InteropServices.Marshal]::SizeOf([type]'LitdGate.JOBOBJECT_EXTENDED_LIMIT_INFORMATION')
    if (-not [LitdGate.JobNative]::SetInformationJobObject(
        $job,
        [LitdGate.JobNative]::JobObjectExtendedLimitInformation,
        [ref]$info,
        $length
    )) {
        $win32 = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        [LitdGate.JobNative]::CloseHandle($job) | Out-Null
        throw "TEST_GATE_JOB_LIMIT_FAILED: SetInformationJobObject win32=$win32"
    }
    $job
}
