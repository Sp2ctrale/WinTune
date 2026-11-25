# Real-machine probe for the WinTune "Context menu" module.
# Run unelevated in Windows PowerShell 5.1:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/context-menu-probe.ps1
# Writes only under HKCU\Software\Classes and HKCU\...\Shell Extensions\Blocked,
# records what existed before and restores everything in `finally`.
# Never calls DoIt() on a verb, never writes under HKLM.
# ASCII-only source: non-ASCII characters are built with [char].

$ErrorActionPreference = 'Stop'
$e = [string][char]0xE9
$label = "WinTune probe $e'"
$folderName = "probe '$e x"
$notepad = 'C:\Windows\System32\notepad.exe'
$shell = New-Object -ComObject Shell.Application
$classes = 'HKCU:\Software\Classes'
$blockedPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked'
$prevVersions = '{596AB062-B4D2-4215-9F74-E9109B0A8153}'

$script:created = New-Object System.Collections.ArrayList   # HKCU keys we created (delete recursively)
$script:results = [ordered]@{}

function Report($name, $ok, $evidence) {
    $script:results[$name] = $ok
    $tag = if ($ok) { 'PASS' } else { 'FAIL' }
    Write-Output ("[{0}] {1}: {2}" -f $tag, $name, $evidence)
}

function Get-VerbNames($item) {
    $names = @()
    foreach ($v in $item.Verbs()) { $names += ($v.Name -replace '&', '') }
    return , $names
}

function Get-ShellItem($path) {
    if ($path -eq 'C:\') {
        $ns = $shell.NameSpace(17)
        foreach ($it in $ns.Items()) { if ($it.Path -eq 'C:\') { return $it } }
        throw 'drive item C:\ not found under This PC'
    }
    $parent = Split-Path -Parent $path
    $leaf = Split-Path -Leaf $path
    return $shell.NameSpace($parent).ParseName($leaf)
}

function New-HkcuKey($rel) {
    $full = Join-Path $classes $rel
    if (Test-Path -LiteralPath $full) { throw "pre-existing key $full" }
    # record the topmost missing ancestor so cleanup removes exactly what we created
    $top = $full
    $parent = Split-Path -Parent $full
    while ($parent -ne $classes -and -not (Test-Path -LiteralPath $parent)) { $top = $parent; $parent = Split-Path -Parent $parent }
    New-Item -Path $full -Force | Out-Null
    [void]$script:created.Add($top)
    return $full
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('wintune-probe-' + [guid]::NewGuid().ToString('N'))
$tmpFolder = Join-Path $tmp $folderName
$tmpFile = Join-Path $tmp ("probe file $e.txt")

# Blocked key state before
$blockedKeyExisted = Test-Path -LiteralPath $blockedPath
$blockedValueExisted = $false
if ($blockedKeyExisted) {
    $blockedValueExisted = $null -ne (Get-ItemProperty -LiteralPath $blockedPath -Name $prevVersions -ErrorAction SilentlyContinue)
}
Write-Output ("Windows: {0}" -f [Environment]::OSVersion.VersionString)
Write-Output ("Blocked key existed: {0}; value existed: {1}" -f $blockedKeyExisted, $blockedValueExisted)

try {
    New-Item -ItemType Directory -Path $tmpFolder -Force | Out-Null
    Set-Content -LiteralPath $tmpFile -Value 'x'

    # ---- Check 1: MUIVerb/Icon/command on Directory, Drive, *
    foreach ($t in @(
            @{ Name = 'Check 1 Directory'; Cls = 'Directory'; Path = $tmpFolder },
            @{ Name = 'Check 1 Drive'; Cls = 'Drive'; Path = 'C:\' },
            @{ Name = 'Check 1 File (*)'; Cls = '*'; Path = $tmpFile })) {
        try {
            $key = New-HkcuKey ("{0}\shell\WinTuneProbe.a" -f $t.Cls)
            # '*' must be literal: use -LiteralPath for everything
            New-ItemProperty -LiteralPath $key -Name 'MUIVerb' -Value $label | Out-Null
            New-ItemProperty -LiteralPath $key -Name 'Icon' -Value ('"' + $notepad + '"') | Out-Null
            $cmdKey = Join-Path $key 'command'
            New-Item -Path $cmdKey -Force | Out-Null
            Set-Item -LiteralPath $cmdKey -Value ('"' + $notepad + '" "%1"')
            $names = Get-VerbNames (Get-ShellItem $t.Path)
            $found = $names -contains $label
            Report $t.Name $found ("label '{0}' in Verbs(): {1}; {2} verbs: {3}" -f $label, $found, $names.Count, ($names -join ' | '))
        } catch {
            Report $t.Name $false ("exception: " + $_.Exception.Message)
        }
    }

    # ---- Check 2: ProgrammaticAccessOnly in HKCU copy hides an HKLM verb; MUIVerb-only copy renames it
    try {
        $item = Get-ShellItem $tmpFolder
        $before = Get-VerbNames $item
        $picked = $null
        $hklmShell = 'HKLM:\SOFTWARE\Classes\Directory\shell'
        foreach ($sk in Get-ChildItem -LiteralPath $hklmShell) {
            $vn = $sk.PSChildName
            if ($vn -like 'WinTuneProbe*') { continue }
            if ($null -ne (Get-ItemProperty -LiteralPath $sk.PSPath -Name 'Extended' -ErrorAction SilentlyContinue)) { continue }
            if (Test-Path -LiteralPath (Join-Path $classes "Directory\shell\$vn")) { continue }
            $key = New-HkcuKey "Directory\shell\$vn"
            New-ItemProperty -LiteralPath $key -Name 'ProgrammaticAccessOnly' -Value '' | Out-Null
            $after = Get-VerbNames (Get-ShellItem $tmpFolder)
            $gone = @($before | Where-Object { $after -notcontains $_ })
            if ($gone.Count -gt 0) { $picked = $vn; $goneNames = $gone; break }
            Remove-Item -LiteralPath $key -Recurse -Force
            [void]$script:created.Remove($key)
        }
        if (-not $picked) {
            Report 'Check 2a ProgrammaticAccessOnly hides HKLM verb' $false 'no HKLM Directory\shell verb disappeared with a ProgrammaticAccessOnly HKCU copy (or none showed)'
            Report 'Check 2b MUIVerb-only HKCU copy renames, command kept' $false 'skipped (no candidate)'
        } else {
            Report 'Check 2a ProgrammaticAccessOnly hides HKLM verb' $true ("verb '{0}' removed from Verbs(): {1}" -f $picked, ($goneNames -join ' | '))
            $key = Join-Path $classes "Directory\shell\$picked"
            Remove-ItemProperty -LiteralPath $key -Name 'ProgrammaticAccessOnly'
            New-ItemProperty -LiteralPath $key -Name 'MUIVerb' -Value 'Probe renamed' | Out-Null
            $after2 = Get-VerbNames (Get-ShellItem $tmpFolder)
            $renamed = $after2 -contains 'Probe renamed'
            $stillOld = @($goneNames | Where-Object { $after2 -contains $_ })
            $hklmCmd = (Get-ItemProperty -LiteralPath (Join-Path $hklmShell "$picked\command")).'(default)'
            $hkcrCmd = (Get-ItemProperty -LiteralPath "Registry::HKEY_CLASSES_ROOT\Directory\shell\$picked\command").'(default)'
            $cmdKept = ($hklmCmd -eq $hkcrCmd) -and -not [string]::IsNullOrEmpty($hkcrCmd)
            Report 'Check 2b MUIVerb-only HKCU copy renames, command kept' ($renamed -and $cmdKept) ("renamed label shown: {0}; old label(s) still shown: {1}; HKLM command: {2}; HKCR merged command: {3}" -f $renamed, ($stillOld -join ' | '), $hklmCmd, $hkcrCmd)
        }
    } catch {
        Report 'Check 2' $false ("exception: " + $_.Exception.Message)
    }

    # ---- Check 3: Blocked value hides a shellex handler
    try {
        $win = Get-ShellItem 'C:\Windows'
        $before3 = Get-VerbNames $win
        if (-not $blockedKeyExisted) { New-Item -Path $blockedPath -Force | Out-Null }
        New-ItemProperty -LiteralPath $blockedPath -Name $prevVersions -Value '' -PropertyType String -Force | Out-Null
        $after3 = Get-VerbNames (Get-ShellItem 'C:\Windows')
        $gone3 = @($before3 | Where-Object { $after3 -notcontains $_ })
        Report 'Check 3 Blocked hides Restore previous versions' ($gone3.Count -gt 0) ("verbs removed from C:\Windows Verbs(): {0}; before {1} after {2}" -f ($gone3 -join ' | '), $before3.Count, $after3.Count)
    } catch {
        Report 'Check 3' $false ("exception: " + $_.Exception.Message)
    }
}
finally {
    # restore Blocked exactly
    try {
        if (-not $blockedValueExisted -and (Test-Path -LiteralPath $blockedPath)) {
            Remove-ItemProperty -LiteralPath $blockedPath -Name $prevVersions -ErrorAction SilentlyContinue
        }
        if (-not $blockedKeyExisted -and (Test-Path -LiteralPath $blockedPath)) {
            Remove-Item -LiteralPath $blockedPath -Recurse -Force
        }
    } catch { Write-Output ("cleanup Blocked error: " + $_.Exception.Message) }
    # delete HKCU keys we created (children first not needed with -Recurse)
    foreach ($k in $script:created) {
        try { if (Test-Path -LiteralPath $k) { Remove-Item -LiteralPath $k -Recurse -Force } }
        catch { Write-Output ("cleanup error on {0}: {1}" -f $k, $_.Exception.Message) }
    }
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
}
