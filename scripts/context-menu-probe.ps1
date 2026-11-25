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
$blockedOldData = $null
$blockedOldKind = $null
if ($blockedKeyExisted) {
    $bk = Get-Item -LiteralPath $blockedPath
    if ($bk.GetValueNames() -contains $prevVersions) {
        $blockedValueExisted = $true
        $blockedOldKind = $bk.GetValueKind($prevVersions)
        $blockedOldData = $bk.GetValue($prevVersions, $null, 'DoNotExpandEnvironmentNames')
    }
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

    # ---- Check 3: Blocked value hides the Restore previous versions handler
    # Target verb is matched by label (accelerators stripped); French or English.
    try {
        $targets = @(('Restaurer les versions pr' + $e + 'c' + $e + 'dentes'), 'Restore previous versions')
        function Get-ChildVerbs {
            # Read in a brand-new process so no in-process handler cache can explain the result.
            $childCode = @'
$p = 'C:\Windows'
$s = New-Object -ComObject Shell.Application
$it = $s.NameSpace((Split-Path -Parent $p)).ParseName((Split-Path -Leaf $p))
foreach ($v in $it.Verbs()) {
    $n = $v.Name.Replace('&', '')
    Write-Output (-join ($n.ToCharArray() | ForEach-Object { if ([int]$_ -gt 126) { '[U+{0:X4}]' -f [int]$_ } else { $_ } }))
}
'@
            $enc = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($childCode))
            $out = & powershell.exe -NoProfile -ExecutionPolicy Bypass -EncodedCommand $enc 2>$null
            return , @($out | Where-Object { $_ -isnot [System.Management.Automation.ErrorRecord] } | ForEach-Object { [string]$_ } | Where-Object { $_ -notlike '#< CLIXML*' -and $_ -notlike '<Objs*' })
        }
        $escTargets = @($targets | ForEach-Object { -join ($_.ToCharArray() | ForEach-Object { if ([int]$_ -gt 126) { '[U+{0:X4}]' -f [int]$_ } else { $_ } }) })
        # Control read in a new process BEFORE writing Blocked (new processes may list fewer verbs on their own).
        $ctrl = Get-ChildVerbs
        $presentBefore = @($ctrl | Where-Object { $escTargets -contains $_ })
        Write-Output ("Check 3 control (new process, before write) ({0}): {1}" -f $ctrl.Count, ($ctrl -join ' | '))
        Write-Output ("Check 3 precondition: target verb present before write: {0} ({1})" -f ($presentBefore.Count -gt 0), ($presentBefore -join ' | '))
        if (-not $blockedKeyExisted) { New-Item -Path $blockedPath -Force | Out-Null }
        New-ItemProperty -LiteralPath $blockedPath -Name $prevVersions -Value '' -PropertyType String -Force | Out-Null
        $childNames = Get-ChildVerbs
        $presentAfter = @($childNames | Where-Object { $escTargets -contains $_ })
        Write-Output ("Check 3 after write (new process) ({0}): {1}" -f $childNames.Count, ($childNames -join ' | '))
        if ($childNames.Count -eq 0 -or $ctrl.Count -eq 0) { throw 'child process returned no verbs' }
        $removed = @($ctrl | Where-Object { $childNames -notcontains $_ })
        $hidden = ($presentBefore.Count -gt 0) -and ($presentAfter.Count -eq 0)
        Report 'Check 3 Blocked hides Restore previous versions (new-process read)' $hidden ("precondition (target present before): {0}; present after: {1}; removed by the write: {2}. Proves only Verbs() from a fresh non-Explorer process; Explorer itself is untested." -f ($presentBefore.Count -gt 0), ($presentAfter.Count -gt 0), ($removed -join ' | '))
    } catch {
        Report 'Check 3' $false ("exception: " + $_.Exception.Message)
    }
}
finally {
    # restore Blocked exactly (value data and kind, key only if we created it)
    try {
        if (Test-Path -LiteralPath $blockedPath) {
            if ($blockedValueExisted) {
                Set-ItemProperty -LiteralPath $blockedPath -Name $prevVersions -Value $blockedOldData -Type $blockedOldKind
            } else {
                Remove-ItemProperty -LiteralPath $blockedPath -Name $prevVersions -ErrorAction SilentlyContinue
            }
            if (-not $blockedKeyExisted) { Remove-Item -LiteralPath $blockedPath -Recurse -Force }
        }
    } catch { Write-Output ("cleanup Blocked error: " + $_.Exception.Message) }
    # delete HKCU keys we created (children first not needed with -Recurse)
    foreach ($k in $script:created) {
        try { if (Test-Path -LiteralPath $k) { Remove-Item -LiteralPath $k -Recurse -Force } }
        catch { Write-Output ("cleanup error on {0}: {1}" -f $k, $_.Exception.Message) }
    }
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
}

# ---- End-of-run cleanup verification
$leftover = $false
foreach ($k in $script:created) {
    $st = if (Test-Path -LiteralPath $k) { $leftover = $true; 'LEFTOVER' } else { 'RESTORED' }
    Write-Output ("{0} {1}" -f $st, $k)
}
if ($blockedKeyExisted) {
    $bk = Get-Item -LiteralPath $blockedPath
    if ($blockedValueExisted) {
        $same = ($bk.GetValueKind($prevVersions) -eq $blockedOldKind) -and ($bk.GetValue($prevVersions, $null, 'DoNotExpandEnvironmentNames') -eq $blockedOldData)
        $st = if ($same) { 'RESTORED' } else { $leftover = $true; 'LEFTOVER' }
    } else {
        $st = if ($bk.GetValueNames() -contains $prevVersions) { $leftover = $true; 'LEFTOVER' } else { 'RESTORED' }
    }
    Write-Output ("{0} {1} value {2}" -f $st, $blockedPath, $prevVersions)
} else {
    $st = if (Test-Path -LiteralPath $blockedPath) { $leftover = $true; 'LEFTOVER' } else { 'RESTORED' }
    Write-Output ("{0} {1} (key did not exist before)" -f $st, $blockedPath)
}
$st = if (Test-Path -LiteralPath $tmp) { $leftover = $true; 'LEFTOVER' } else { 'RESTORED' }
Write-Output ("{0} {1}" -f $st, $tmp)
if ($leftover) { exit 1 }
