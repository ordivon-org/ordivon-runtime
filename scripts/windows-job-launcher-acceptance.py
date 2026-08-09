#!/usr/bin/env python3
from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
PUBLIC = Path('/mnt/c/Users/Public')
CSC = Path('/mnt/c/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe')
POWERSHELL = Path('/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe')


def fail(message: str) -> None:
    raise RuntimeError(message)


def windows_path(path: Path) -> str:
    resolved = path.resolve()
    text = str(resolved)
    prefix = '/mnt/c/'
    if not text.lower().startswith(prefix):
        fail(f'acceptance path must be on C: for Windows compilation: {text}')
    return 'C:\\' + text[len(prefix):].replace('/', '\\')


def compile_cs(source: Path, output: Path) -> None:
    command = [
        str(CSC),
        '/nologo',
        '/optimize+',
        f'/out:{windows_path(output)}',
        windows_path(source),
    ]
    completed = subprocess.run(command, text=True, errors='replace', capture_output=True, timeout=20)
    if completed.returncode != 0:
        fail(f'csc failed for {source.name}: {completed.stdout}{completed.stderr}')


def launcher_args(launcher: Path, target: Path, *options: str, target_args: list[str]) -> list[str]:
    return [
        str(launcher),
        '--executable', windows_path(target),
        '--cwd', windows_path(target.parent),
        *options,
        '--',
        *target_args,
    ]


def run(command: list[str], *, timeout: float = 20) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, text=True, capture_output=True, timeout=timeout)


def decode_field(stdout: str, name: str) -> str:
    prefix = name + '='
    for line in stdout.splitlines():
        if line.startswith(prefix):
            return base64.b64decode(line[len(prefix):]).decode('utf-8')
    fail(f'missing field {name}')


def cpu_ms(stdout: str) -> float:
    match = re.search(r'\bcpuMs=([0-9.]+)', stdout)
    if not match:
        fail('CPU fixture omitted cpuMs')
    return float(match.group(1))


def marker_process_count(marker: str) -> int:
    escaped = marker.replace("'", "''")
    command = (
        "$m='" + escaped + "'; "
        "$rows=Get-CimInstance Win32_Process | "
        "Where-Object {$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}; "
        "Write-Output @($rows).Count"
    )
    completed = subprocess.run(
        [str(POWERSHELL), '-NoProfile', '-Command', command],
        text=True,
        capture_output=True,
        timeout=10,
    )
    if completed.returncode != 0:
        fail('cannot query marker processes: ' + completed.stderr)
    return int(completed.stdout.strip() or '0')


def main() -> int:
    if not CSC.exists() or not POWERSHELL.exists() or not PUBLIC.exists():
        print('SKIP: Windows/WSL acceptance prerequisites are unavailable')
        return 0

    temp = PUBLIC / f'ordivon-runtime-windows-acceptance-{os.getpid()}'
    temp.mkdir(parents=True, exist_ok=False)
    summary: dict[str, object] = {}
    try:
        launcher_src = temp / 'Ordivon.WindowsJobLauncher.cs'
        fixture_src = temp / 'Ordivon.WindowsJobFixture.cs'
        shutil.copyfile(ROOT / 'platform/windows/Ordivon.WindowsJobLauncher.cs', launcher_src)
        shutil.copyfile(ROOT / 'platform/windows/Ordivon.WindowsJobFixture.cs', fixture_src)
        launcher = temp / 'ordivon-windows-job.exe'
        fixture = temp / 'ordivon-windows-job-fixture.exe'
        compile_cs(launcher_src, launcher)
        compile_cs(fixture_src, fixture)
        spaced_dir = temp / 'space dir'
        spaced_dir.mkdir()
        spaced_fixture = spaced_dir / 'fixture with spaces.exe'
        shutil.copyfile(fixture, spaced_fixture)

        context_names = [
            'APPDATA', 'CommonProgramFiles', 'CommonProgramW6432', 'COMPUTERNAME', 'ComSpec',
            'HOMEDRIVE', 'HOMEPATH', 'LOCALAPPDATA', 'NUMBER_OF_PROCESSORS', 'OS', 'Path',
            'PATHEXT', 'PROCESSOR_ARCHITECTURE', 'ProgramData', 'ProgramFiles', 'ProgramW6432',
            'PUBLIC', 'SystemDrive', 'SystemRoot', 'TEMP', 'TMP', 'USERDOMAIN', 'USERNAME',
            'USERPROFILE', 'windir',
        ]
        context_command = [str(launcher), '--describe-runtime-context']
        for name in context_names:
            context_command.extend(['--context-env', name])
        context = run(context_command)
        if context.returncode != 0:
            fail('limited runtime context probe failed: ' + context.stderr)
        try:
            context_value = json.loads(context.stdout)
        except json.JSONDecodeError as error:
            fail(f'limited runtime context probe returned invalid JSON: {error}')
        admin_attrs = int(context_value['administratorsGroupAttributes'])
        if context_value.get('tokenType') != 1:
            fail('limited runtime context token is not primary')
        if context_value.get('tokenIsElevated') is not False:
            fail('limited runtime context token is elevated')
        if int(context_value.get('tokenIntegrityLevelRid', 0)) > 8192:
            fail('limited runtime context token exceeds Medium integrity')
        if admin_attrs != 0xFFFFFFFF and (admin_attrs & 0x4) != 0 and (admin_attrs & 0x10) == 0:
            fail('limited runtime context leaves Administrators enabled')
        if context_value.get('tokenSelection') == 'lua_medium_filtered' and admin_attrs != 0xFFFFFFFF and (admin_attrs & 0x10) == 0:
            fail('LUA-derived runtime token did not make Administrators deny-only')
        context_env = context_value.get('environment', {})
        for required in ['SystemRoot', 'Path', 'PATHEXT', 'USERPROFILE', 'TEMP', 'TMP', 'APPDATA', 'LOCALAPPDATA']:
            if not context_env.get(required):
                fail(f'limited runtime context omitted required environment {required}')
        for forbidden in ['PNPM_HOME', 'OneDrive', 'WSL_DISTRO_NAME', 'PSModulePath']:
            if any(name.lower() == forbidden.lower() for name in context_env):
                fail(f'limited runtime context leaked non-baseline environment {forbidden}')
        summary['limitedContext'] = {
            'tokenSelection': context_value['tokenSelection'],
            'tokenIsElevated': context_value['tokenIsElevated'],
            'integrityLevelRid': context_value['tokenIntegrityLevelRid'],
            'administratorsGroupAttributes': admin_attrs,
            'environmentKeys': len(context_env),
        }

        elevated_command = [str(launcher), '--describe-runtime-context', '--authority', 'elevated']
        for name in context_names:
            elevated_command.extend(['--context-env', name])
        elevated_context = run(elevated_command)
        if elevated_context.returncode != 0:
            fail('elevated runtime context probe failed: ' + elevated_context.stderr)
        try:
            elevated_value = json.loads(elevated_context.stdout)
        except json.JSONDecodeError as error:
            fail(f'elevated runtime context probe returned invalid JSON: {error}')
        elevated_admin_attrs = int(elevated_value['administratorsGroupAttributes'])
        if elevated_value.get('tokenSelection') != 'current_elevated':
            fail('elevated context did not select the current elevated provider token')
        if elevated_value.get('tokenType') != 1 or elevated_value.get('tokenIsElevated') is not True:
            fail('elevated context did not prove an elevated primary token')
        if int(elevated_value.get('tokenIntegrityLevelRid', 0)) < 12288:
            fail('elevated context is below High integrity')
        if elevated_admin_attrs == 0xFFFFFFFF or (elevated_admin_attrs & 0x4) == 0 or (elevated_admin_attrs & 0x10) != 0:
            fail('elevated context did not prove Administrators enabled')
        if elevated_value.get('tokenUserSid') != context_value.get('tokenUserSid'):
            fail('limited and elevated contexts changed user SID')
        if elevated_value.get('environment') != context_env:
            fail('requested authority changed the frozen baseline environment')
        summary['elevatedContext'] = {
            'tokenSelection': elevated_value['tokenSelection'],
            'tokenIsElevated': elevated_value['tokenIsElevated'],
            'integrityLevelRid': elevated_value['tokenIntegrityLevelRid'],
            'administratorsGroupAttributes': elevated_admin_attrs,
            'sameUserSid': True,
            'sameEnvironment': True,
        }

        normal = run(launcher_args(
            launcher, spaced_fixture, '--diagnostics',
            target_args=['normal', 'accept-normal', '23'],
        ))
        if normal.returncode != 23:
            fail(f'normal exit mismatch: {normal.returncode}')
        if 'W1_FIXTURE_STDOUT marker=accept-normal' not in normal.stdout:
            fail('normal stdout was not preserved')
        if 'W1_FIXTURE_STDERR marker=accept-normal' not in normal.stderr:
            fail('normal stderr was not preserved')
        summary['normal'] = {'exitCode': normal.returncode, 'spacedExecutableAndCwd': True}

        echo_args = ['plain', 'two words', 'quote"inside', 'trailing\\', '', 'back\\slash and "quote"']
        echo_env = 'alpha beta="gamma"\\delta'
        echo = run(launcher_args(
            launcher,
            fixture,
            '--inherit-environment', 'false',
            '--env', 'W1_ENV=' + echo_env,
            target_args=['echo', *echo_args],
        ))
        if echo.returncode != 0:
            fail('echo acceptance failed: ' + echo.stderr)
        if decode_field(echo.stdout, 'W1_ECHO_ENV_B64') != echo_env:
            fail('environment round-trip mismatch')
        if decode_field(echo.stdout, 'W1_ECHO_SYSTEMROOT_B64') != '<null>':
            fail('clear-environment contract leaked SystemRoot')
        for index, expected in enumerate(echo_args):
            actual = decode_field(echo.stdout, f'W1_ECHO_ARG_{index}_B64')
            if actual != expected:
                fail(f'argv round-trip mismatch at {index}: {actual!r} != {expected!r}')
        summary['argvEnv'] = {'args': len(echo_args), 'clearEnvironment': True}

        memory_control = run(launcher_args(
            launcher, fixture,
            target_args=['memory', 'accept-memory-control', '128'],
        ))
        if memory_control.returncode != 0 or 'W1_MEM_ALLOCATED' not in memory_control.stdout:
            fail('memory control could not allocate 128 MiB')
        memory_limited = run(launcher_args(
            launcher, fixture,
            '--memory-max-bytes', str(64 * 1024 * 1024),
            target_args=['memory', 'accept-memory-limit', '128'],
        ))
        if memory_limited.returncode != 42 or 'W1_MEM_BLOCKED' not in memory_limited.stderr:
            fail('job memory limit did not block the control allocation')
        summary['memory'] = {'controlExit': 0, 'limitedExit': 42, 'limitBytes': 64 * 1024 * 1024}

        process_limit = run(launcher_args(
            launcher, fixture,
            '--active-process-limit', '1',
            target_args=['process-limit', 'accept-process-limit'],
        ))
        if process_limit.returncode != 0 or 'W1_PROCESS_LIMIT_BLOCKED' not in process_limit.stdout:
            fail('active process limit did not block child creation')
        summary['activeProcessLimit'] = {'limit': 1}

        cpu_control = run(launcher_args(
            launcher, fixture,
            target_args=['cpu', 'accept-cpu-control', '3000'],
        ), timeout=10)
        cpu_limited = run(launcher_args(
            launcher, fixture,
            '--cpu-quota-percent', '25',
            target_args=['cpu', 'accept-cpu-limit', '3000'],
        ), timeout=10)
        control_cpu = cpu_ms(cpu_control.stdout)
        limited_cpu = cpu_ms(cpu_limited.stdout)
        if cpu_control.returncode != 0 or cpu_limited.returncode != 0:
            fail('CPU acceptance process failed')
        if control_cpu < 2000:
            fail(f'CPU control was unexpectedly weak: {control_cpu} ms')
        if limited_cpu >= control_cpu * 0.65:
            fail(f'CPU hard cap did not reduce CPU time enough: {limited_cpu} vs {control_cpu}')
        summary['cpu'] = {'controlCpuMs': control_cpu, 'limitedCpuMs': limited_cpu, 'quotaPercent': 25}

        marker = f'ORDIVON_W1_ACCEPT_TREE_{os.getpid()}'
        tree_command = launcher_args(
            launcher,
            fixture,
            '--diagnostics',
            target_args=['tree', marker],
        )
        process = subprocess.Popen(tree_command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            ready = process.stderr.readline() if process.stderr is not None else ''
            if 'ORDIVON_WINDOWS_JOB_READY' not in ready:
                fail('tree acceptance never reached launcher readiness: ' + ready)
            time.sleep(0.5)
            if marker_process_count(marker) < 2:
                fail('tree fixture did not create descendants before cancellation')
            process.kill()
            process.wait(timeout=5)
            time.sleep(0.7)
            remaining = marker_process_count(marker)
            if remaining != 0:
                fail(f'Windows descendant processes survived launcher death: {remaining}')
            summary['killOnClose'] = {'remainingAfterLauncherKill': remaining}
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            if process.stdout is not None:
                process.stdout.close()
            if process.stderr is not None:
                process.stderr.close()

        print(json.dumps({'status': 'pass', 'summary': summary}, sort_keys=True))
        return 0
    finally:
        shutil.rmtree(temp, ignore_errors=True)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f'FAIL: {error}', file=sys.stderr)
        raise SystemExit(1)
