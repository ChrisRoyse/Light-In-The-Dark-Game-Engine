# Windows Packaging

Windows is the only supported product and packaging target. macOS, Linux, and WSL product paths are explicitly parked and are not fallback build or runtime paths.

Build a deterministic release payload and ZIP:

```powershell
.\scripts\Set-GameEditorCalyxRuntimeEnv.ps1
.\scripts\package-windows.ps1 -NoInstaller
```

Pass `-PackageVersion` as a canonical three-part numeric version when producing
a release. The default is `0.0.1`; major/minor components must be at most 255
and patch at most 65535 so the same value is valid Windows Installer metadata.

Build an MSI when WiX v4 is installed:

```powershell
dotnet tool install --global wix --version 4.0.6
.\scripts\package-windows.ps1 -BuildInstaller -WixPath wix
```

Every MSI build is independently opened through the native Windows Installer
database API after any signing step. Packaging fails unless its x64/per-machine
identity, version, upgrade/downgrade policy, directories, components, feature
membership, seven file rows, embedded cabinet, and both ORT environment rows
match the package contract. The embedded cabinet is then extracted with the
Windows `expand.exe` utility and every extracted byte count and SHA-256 is
compared with the staged payload. The final MSI byte count, SHA-256, table
readback, cabinet identity, and payload hashes are persisted in
`msi-readback.json` and included in `package-readback.json`.
The same verifier requires the authored Start Menu folder, editor/game
shortcuts, embedded product icon bytes, uninstall folder cleanup, and ARP
comments/help/product/install-location properties. The icon is reproducibly
generated from code-native drawing primitives with:

```powershell
.\scripts\New-WindowsProductIcon.ps1
```

The same fail-loud verification can be rerun without installing the product:

```powershell
.\scripts\verify-windows-msi.ps1 `
  -MsiPath .\dist\windows\litd-windows-x64.msi `
  -StageRoot .\dist\windows\stage `
  -ExpectedPackageVersion 0.0.1
```

Run install, repair, downgrade-refusal, and uninstall acceptance from an
explicitly elevated 64-bit PowerShell session with two constructed MSIs:

```powershell
.\scripts\verify-windows-msi-lifecycle.ps1 `
  -NewerMsiPath .\target\fsv\issue711-newer\litd-windows-x64.msi `
  -NewerStageRoot .\target\fsv\issue711-newer\stage `
  -NewerVersion 1.2.3 `
  -OlderMsiPath .\target\fsv\issue711-older\litd-windows-x64.msi `
  -OlderStageRoot .\target\fsv\issue711-older\stage `
  -OlderVersion 1.2.2 `
  -LogRoot .\target\fsv\issue711-lifecycle
```

The lifecycle harness never self-elevates. Before mutation it independently
verifies both MSI payloads and refuses to run when a related product, the
install directory, or either system ORT environment value already exists. It
installs the newer MSI, hashes every installed file, changes the installed
README, requires repair to restore all hashes, requires the older MSI to fail
with the authored downgrade message, then uninstalls and proves product,
directory, and environment cleanup. Verbose `msiexec` logs and a typed
`litd.windows.msi-lifecycle.v1` readback remain under the requested log root.
On an intermediate failure it attempts to uninstall only related products
created after the clean preflight and fails loud if that cleanup cannot finish.

Verify the portable staged payload without installing it by providing a real
packaged game:

```powershell
.\scripts\verify-windows-portable-launch.ps1 `
  -StageRoot .\dist\windows\stage `
  -GamePackagePath C:\games\sample.litdgame.json `
  -OutputPath .\dist\windows\portable-launch-readback.json
```

The verifier first requires the exact schema-v2 six-file manifest set and
independently checks every staged byte count/hash, both PE subsystem rows, and
the duplicated ONNX Runtime identity. It launches the editor and game
sequentially with process-local ORT values that point only inside the portable
`app` directory. Each process must remain alive, create a real main HWND with
the authored title, and retain it for the observation window. The verifier
records binary/package/ORT hashes, PID/HWND/title/timing evidence, host video
controllers, and Vulkan-loader presence in
`litd.windows.portable-launch.v1`, then closes or kills only processes it
started. This is launch evidence, not a substitute for manual visual FSV.

Code signing is optional, explicit, and fail-loud:

```powershell
.\scripts\package-windows.ps1 -BuildInstaller `
  -SignToolPath "C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe" `
  -CertificateThumbprint "<sha1-thumbprint>"
```

`-SignToolPath` and `-CertificateThumbprint` must be provided together. When
both are omitted, the readback records explicit `unsigned` status. When both
are present, packaging signs the staged editor and game before writing the
manifest, ZIP, or MSI; each signature must then pass
`signtool verify /pa /all /v`. After WiX construction, the final MSI is signed
and verified the same way before its database and cabinet are inspected. A
missing tool, malformed 40-hex thumbprint, invalid timestamp URL, signing
failure, verification failure, or partially supplied configuration stops
packaging with a stable `LITD_WINDOWS_SIGNING_*` error.

ZIP does not carry an Authenticode signature and is never reported as signed.
Packaging instead proves that its editor/game entries are byte-identical to
the staged binaries whose Authenticode status was just established. The final
ZIP byte count and SHA-256 are then bound by `release-manifest.json`. This
distinction is recorded in the `litd.windows.signing.v1` readback.

Exercise the configuration, exact SignTool arguments, unsigned readback, ZIP
binary identity, and fail-loud edges without a certificate or signing call:

```powershell
.\scripts\test-windows-signing-contract.ps1
```

The script stages `litd-editor.exe`, `litd-game.exe`, `README.md`, `Cargo.lock`,
`licenses/Ubuntu-Font-License-1.0.txt`, and `package-manifest.json`, then writes
`package-readback.json` with package hashes, the exact runtime-font notice
identity, and signing/installer status. The notice must remain 4,673 bytes with
SHA-256
`2f0015108d68627bd788d313f529c21ff4da2c2c42a5e1f3883acc83480f9002`;
packaging fails before producing an artifact when it is absent or changed.

Packaging also requires the canonical Calyx `onnxruntime.dll`. Bootstrap
`ORT_DYLIB_PATH` with `Set-GameEditorCalyxRuntimeEnv.ps1`, or pass the same
canonical absolute path through `-OrtDylibPath`. The script fails loud when the
DLL is absent, outside `C:\code\GameEditor\Calyx`, or named incorrectly. The
payload records its SHA-256 and version, and the MSI owns
`ORT_DYLIB_PATH`/`CALYX_ORT_CAPI` environment values that point at the installed
application directory.

The package command writes a strict `litd.windows.release.v2`
`release-manifest.json` after every requested artifact has been constructed and
optionally signed. A `-NoInstaller` package declares one `portable_zip` artifact
and selects it as primary. A successful `-BuildInstaller` package declares both
`portable_zip` and `windows_msi` artifacts and selects `windows_msi` as primary.
Every row binds the exact leaf filename, byte count, and SHA-256. Check a local
or self-hosted candidate before presenting it as an update:

```powershell
.\scripts\verify-windows-local-update.ps1 `
  -ReleaseManifestPath .\dist\windows\release-manifest.json `
  -CurrentVersion 0.0.0
```

The verifier returns `update_available` only after validating the v2 schema,
artifact kinds, unique primary selection, and independently hashing every
declared artifact. Its readback identifies the selected primary and all verified
rows. Equal versions return `current`; an older candidate, missing artifact,
invalid manifest, or changed ZIP/MSI fails loud with a stable
`LITD_WINDOWS_UPDATE_*` code. It never installs or launches an artifact.
