[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$PackageDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProfilePath = Join-Path $PackageDir 'profile.json'
if (-not (Test-Path -LiteralPath $ProfilePath)) { throw 'profile.json nao foi encontrado neste pacote.' }
$Profile = Get-Content -LiteralPath $ProfilePath -Raw | ConvertFrom-Json

if ($Profile.relayUrl -notmatch '^wss://[A-Za-z0-9.-]+(?::\d{1,5})?$') { throw 'Endereco WSS invalido no perfil.' }
if ($Profile.accessKey -notmatch '^[A-Za-z0-9_-]{43}$') { throw 'Chave do dispositivo invalida no perfil.' }
if ($Profile.installerUrl -notmatch '^https://') { throw 'O instalador precisa vir por HTTPS.' }

$Work = Join-Path $env:TEMP ("coucou-managed-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    $Uri = [Uri]$Profile.installerUrl
    $Extension = [IO.Path]::GetExtension($Uri.AbsolutePath)
    if ($Extension -notin @('.exe', '.msi')) { throw 'O instalador deve ser .exe ou .msi.' }
    $Installer = Join-Path $Work ("Coucou-installer" + $Extension)
    Write-Host 'Baixando o Coucou oficial...'
    Invoke-WebRequest -Uri $Profile.installerUrl -OutFile $Installer -UseBasicParsing
    if ($Profile.installerSha256) {
        $Actual = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($Actual -ne ([string]$Profile.installerSha256).ToLowerInvariant()) { throw 'O SHA-256 do instalador nao confere.' }
    }

    Get-Process -Name coucou -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Host 'Instalando o Coucou para este usuario...'
    if ($Extension -eq '.msi') {
        $Process = Start-Process msiexec.exe -ArgumentList @('/i', $Installer, '/qn', '/norestart') -Wait -PassThru
    } else {
        $Process = Start-Process $Installer -ArgumentList '/S' -Wait -PassThru
    }
    if ($Process.ExitCode -notin @(0, 3010)) { throw "O instalador terminou com o codigo $($Process.ExitCode)." }

    $SettingsDir = Join-Path $env:APPDATA 'Coucou'
    $SettingsPath = Join-Path $SettingsDir 'settings.json'
    New-Item -ItemType Directory -Path $SettingsDir -Force | Out-Null
    $Settings = [ordered]@{}
    if (Test-Path -LiteralPath $SettingsPath) {
        Copy-Item -LiteralPath $SettingsPath -Destination ($SettingsPath + '.before-managed-install.bak') -Force
        $Existing = Get-Content -LiteralPath $SettingsPath -Raw | ConvertFrom-Json
        foreach ($Property in $Existing.PSObject.Properties) { $Settings[$Property.Name] = $Property.Value }
    }
    $Settings['phoneRelay'] = $true
    $Settings['phoneRelayUrl'] = [string]$Profile.relayUrl
    $Json = $Settings | ConvertTo-Json -Depth 20
    [IO.File]::WriteAllText($SettingsPath, $Json, (New-Object Text.UTF8Encoding($false)))

    if (-not ('CoucouCredentialWriter' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CoucouCredentialWriter {
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  private struct CREDENTIAL {
    public UInt32 Flags; public UInt32 Type; public string TargetName; public string Comment;
    public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten; public UInt32 CredentialBlobSize;
    public IntPtr CredentialBlob; public UInt32 Persist; public UInt32 AttributeCount;
    public IntPtr Attributes; public string TargetAlias; public string UserName;
  }
  [DllImport("advapi32.dll", EntryPoint="CredWriteW", CharSet=CharSet.Unicode, SetLastError=true)]
  private static extern bool CredWrite(ref CREDENTIAL credential, UInt32 flags);
  public static void Write(string target, string user, string secret) {
    IntPtr blob = Marshal.StringToCoTaskMemUni(secret);
    try {
      var c = new CREDENTIAL { Type=1, TargetName=target, UserName=user, Persist=2,
        CredentialBlobSize=(UInt32)(secret.Length * 2), CredentialBlob=blob };
      if (!CredWrite(ref c, 0)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    } finally { Marshal.ZeroFreeCoTaskMemUnicode(blob); }
  }
}
'@
    }
    [CoucouCredentialWriter]::Write('phone-relay-access.fr.louisraille.coucou', 'phone-relay-access', [string]$Profile.accessKey)

    $Candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Coucou\coucou.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Coucou\coucou.exe')
    )
    $Executable = $Candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $Executable) { throw 'Coucou foi instalado, mas coucou.exe nao foi localizado.' }
    Start-Process $Executable
    Start-Sleep -Seconds 4
    if (-not (Get-Process -Name coucou -ErrorAction SilentlyContinue)) { throw 'Coucou nao permaneceu aberto depois da instalacao.' }

    Remove-Item -LiteralPath $ProfilePath -Force
    Write-Host "Pronto: $($Profile.deviceName) esta conectado a $($Profile.relayUrl)." -ForegroundColor Green
    Write-Host 'No Coucou, mostre o codigo de pareamento e escaneie-o no celular.'
}
finally {
    if (Test-Path -LiteralPath $Work) { Remove-Item -LiteralPath $Work -Recurse -Force }
}

