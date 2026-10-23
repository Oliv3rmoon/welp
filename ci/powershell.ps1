# The PowerShell flow a person goes through: set up welp with the `fuck`
# alias, have a mistyped command in history, run `fuck --yeah`, and check
# the fixed command ran.
$ErrorActionPreference = 'Stop'
$env:NO_COLOR = '1'
$env:XDG_STATE_HOME = Join-Path ([IO.Path]::GetTempPath()) "welp-ps-state"
Invoke-Expression (& welp init powershell --alias fuck | Out-String)
if (-not (Get-Command fuck -CommandType Function)) { throw 'the fuck function was not defined' }
$now = Get-Date
Add-History -InputObject ([pscustomobject]@{
    CommandLine = 'gti --version'; ExecutionStatus = 'Completed'
    StartExecutionTime = $now; EndExecutionTime = $now
})
$out = fuck --yeah | Out-String
Write-Output $out
if ($out -notmatch 'git version \d') { throw "fuck --yeah did not run git --version" }
Write-Output '== powershell: fixed and ran git --version'
