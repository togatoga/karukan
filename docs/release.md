# リリースの手順

リリースは GitHub Actions がほぼすべて行います。メンテナの操作は 2 回です。

1. **Actions → Prepare release → Run workflow** で、上げ幅を選んで実行する

   | 上げ幅 | 0.1.0 からの次の番号 | 使いどき |
   |---|---|---|
   | patch | 0.1.1 | 不具合の修正だけ |
   | minor | 0.2.0 | 新機能を入れた |
   | major | 1.0.0 | 互換性が変わる節目 |

   main の CI がすべて通っていることを確かめてから、番号を上げる PR（`chore: release v0.2.0`）を作ります。PR にはリリースノートの下書きが付きます。
2. **その PR を確認してマージする**

   マージすると、`v0.2.0` のタグとリリースページが作られ、前のリリースの `dict.tgz` が添付されます。

## 自動で行われること

- 番号はルートの `Cargo.toml` の `[workspace.package] version` の 1 か所だけです。`scripts/version.sh` がそこを書き換え、`Cargo.lock` とソフトウェアセンター向けの AppStream のリリース一覧も更新します。fcitx5 アドオンや macOS アプリの番号はビルド時にここから入ります。
- タグはリリース PR をマージした main のコミットに付きます。タグと `Cargo.toml` の番号がずれることはありません。
- 辞書（`dict.tgz`）は手元で作っているので、リリースのたびに前のリリースのものを引き継ぎます。`make install` などは `releases/latest` から辞書を取るため、最新のリリースには必ず辞書が付いている必要があります。

## 最初に一度だけ必要な設定

ワークフローが PR を作れるように、**Settings → Actions → General → Workflow permissions** で「Allow GitHub Actions to create and approve pull requests」を有効にします。

## 辞書を作り直したとき

新しい `dict.tgz` は、リリースの後でリリースページから差し替えます。次のリリースからは、差し替えた辞書が引き継がれます。
