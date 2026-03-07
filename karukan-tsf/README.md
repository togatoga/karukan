# karukan-tsf

Windows向け日本語IME。TSF (Text Services Framework) 上で動作し、karukan-engineによるニューラルかな漢字変換を行います。

## Build

### Prerequisites

- Rust stable toolchain
- Windows 10/11 (MSVC ターゲット)

### Native build (Windows MSVC)

llama.cpp (CMake) がデフォルトで動的CRT (`/MD`) を使用するため、Rustのデフォルト静的CRT (`/MT`) と一致させる必要があります:

```powershell
set CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
cargo build -p karukan-tsf --release
```

### Cross-compile from Linux

```bash
# mingw-w64 toolchain が必要
cargo build -p karukan-tsf --release --target x86_64-pc-windows-gnu
```

### Registration

ビルド後、管理者権限のコマンドプロンプトでDLLを登録します:

```powershell
regsvr32 target\release\karukan_tsf.dll
```

登録解除:

```powershell
regsvr32 /u target\release\karukan_tsf.dll
```

## License

MIT OR Apache-2.0
