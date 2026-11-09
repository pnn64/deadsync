$ErrorActionPreference='Stop'
$stage=Join-Path $PSScriptRoot 'song-live-visuals-regressions-v2-song-linux-debug'
$log=Get-Content -LiteralPath (Join-Path $stage 'run.log') -Raw
$results=[regex]::Matches($log,'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;')
$passed=0;$failed=0
foreach($result in $results){$passed+=[int]$result.Groups[1].Value;$failed+=[int]$result.Groups[2].Value}
$targets=[regex]::Matches($log,'Running tests/([^\s]+)\.rs')
$current=if($targets.Count){$targets[$targets.Count-1].Groups[1].Value}else{'compiling'}
$lines=$log.TrimEnd().Split("`n")
[ordered]@{completed_targets=$results.Count;tests_passed=$passed;tests_failed=$failed;current_target=$current;last_line=$lines[$lines.Length-1].TrimEnd()} | ConvertTo-Json
