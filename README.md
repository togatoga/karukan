<div align="center">
  <img src="icon.png" width="128" alt="karukan" />
  <h1>Karukan</h1>
  <p>Linux・macOS向け日本語入力システム — ニューラルかな漢字変換エンジン</p>

  [![CI (engine)](https://github.com/togatoga/karukan/actions/workflows/karukan-engine-ci.yml/badge.svg)](https://github.com/togatoga/karukan/actions/workflows/karukan-engine-ci.yml)
  [![CI (im)](https://github.com/togatoga/karukan/actions/workflows/karukan-im-ci.yml/badge.svg)](https://github.com/togatoga/karukan/actions/workflows/karukan-im-ci.yml)
  [![CI (fcitx5)](https://github.com/togatoga/karukan/actions/workflows/karukan-fcitx5-ci.yml/badge.svg)](https://github.com/togatoga/karukan/actions/workflows/karukan-fcitx5-ci.yml)
  [![CI (macos)](https://github.com/togatoga/karukan/actions/workflows/karukan-macos-ci.yml/badge.svg)](https://github.com/togatoga/karukan/actions/workflows/karukan-macos-ci.yml)
  [![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
</div>

<div align="center">
  <img src="images/demo.gif" width="800" alt="karukan demo" />
</div>

## プロジェクト構成

| クレート | 説明 |
|---------|------|
| [karukan-fcitx5](karukan-fcitx5/) | Linux向けIMEフロントエンド — fcitx5アドオン + C FFI |
| [karukan-macos](karukan-macos/) | macOS向けIMEフロントエンド — Swift/InputMethodKit |
| [karukan-im](karukan-im/) | 共有IMEエンジン — ステートマシン、ローマ字変換、karukan-imserver(macOS向けJSON-RPCサーバー) |
| [karukan-engine](karukan-engine/) | コアライブラリ — ローマ字→ひらがな変換 + llama.cppによるニューラルかな漢字変換 |
| [karukan-cli](karukan-cli/) | CLIツール・サーバー — 辞書ビルド、Sudachi辞書生成、辞書ビューア、AJIMEE-Bench、HTTPサーバー |

## 特徴

- **ニューラルかな漢字変換**: GPT-2ベースのモデルをllama.cppで推論し、高度な日本語変換
- **ライブ変換**: 入力と同時に変換結果をリアルタイム表示。Spaceを押さずに変換が進む（`Ctrl+Shift+L` でON/OFF）
- **コンテキスト対応**: 周辺テキストを考慮した日本語変換
- **変換学習**: ユーザーが選択した変換結果を記憶し、次回以降の変換で優先表示。予測変換（前方一致）にも対応し、入力途中でも学習済みの候補を提示
- **システム辞書**: [SudachiDict](https://github.com/WorksApplications/SudachiDict)の辞書データからシステム辞書を構築
- **候補リライター (Mozcから移植)**: 半角カタカナ、英字の大文字小文字・全角半角、記号の関連候補、数字の各種表記（漢数字・大字・ローマ数字・丸数字・16/8/2進数）を自動生成。各候補にはMozc由来の注釈（「半角カタカナ」「16進数」など）が付く
- **絵文字入力**: かな読み（`ぴえん` → 🥺、`きんにく` → 💪）と Slack 風 `:trigger` クエリ（`:smile` → 😄、`:halo` → 😇）の両方をサポート

> **Note:** 初回起動時にHugging Faceからモデルをダウンロードするため、初回の変換開始までに時間がかかります。2回目以降はダウンロード済みのモデルが使用されます。

## インストール

- **Linux (fcitx5)**: [karukan-fcitx5 の README](karukan-fcitx5/README.md#install) を参照
- **macOS**: [karukan-macos の README](karukan-macos/README.md) を参照

## ライセンス

MIT OR Apache-2.0 のデュアルライセンスで提供しています。

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

[karukan-engine/data/](karukan-engine/data/) 配下には [Mozc](https://github.com/google/mozc)（Google製日本語入力システム）から派生したデータを含み、こちらは [BSD 3-Clause License](http://opensource.org/licenses/BSD-3-Clause) のもとで配布されています。各派生ファイルの由来およびMozcの著作権表記は [THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES) を参照してください。
