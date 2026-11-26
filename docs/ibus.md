# IBus で使う（試作）

GNOME（Wayland）の標準の入力メソッドフレームワークである IBus の上で、fcitx5 を使わずに karukan を動かすためのエンジンです。ibus-daemon はディストリビューションのものをそのまま使い、karukan はその上で動くエンジンのプロセス（`karukan-ibus`）になります。

まだ試作です。fcitx5 版との違いは[最後の節](#fcitx5-版との違い)にまとめてあります。キーバインド・設定・辞書・学習は fcitx5 版と同じで、設定ファイル（`~/.config/karukan-im/config.toml`）とデータ（`~/.local/share/karukan-im/`）も共通です。

## ビルド

llama.cpp のビルドに C/C++ のツールが要ります（fcitx5 の開発パッケージは要りません）。

```bash
sudo apt install ibus cmake make gcc g++ pkg-config clang libclang-dev libssl-dev
cargo build --release -p karukan-ibus
```

## インストール

エンジンの実行ファイルと、IBus にエンジンを知らせる component ファイル（[`karukan-im/ibus/karukan.xml`](../karukan-im/ibus/karukan.xml)）を置きます。IBus は component ファイルをシステムのディレクトリからしか読まないので sudo が要ります。

```bash
sudo install -Dm755 target/release/karukan-ibus /usr/libexec/karukan-ibus
sudo install -Dm644 karukan-im/ibus/karukan.xml /usr/share/ibus/component/karukan.xml
ibus restart
```

`ibus list-engine | grep karukan` で `karukan - Karukan` が出れば IBus に認識されています。エンジンのプロセスは、karukan に切り替えたときに ibus-daemon が `karukan-ibus --ibus` として起動します。

component ファイルは `karukan-ibus --xml` が出力するものと同じです（中身はコードから生成していて、テストで一致を確かめています）。

外すときは2つのファイルを消して `ibus restart` します。

```bash
sudo rm /usr/libexec/karukan-ibus /usr/share/ibus/component/karukan.xml
ibus restart
```

### sudo なしで試す

ibus-daemon が動いている状態で、ビルドした `karukan-ibus` を引数なしで起動すると、自分で IBus に登録します（`RegisterComponent`）。そのまま切り替えれば使えます。

```bash
target/release/karukan-ibus &
ibus engine karukan
```

この登録はプロセスが動いている間だけ有効です。`ibus list-engine` や GNOME の設定の一覧には出ません（IBus がそこに載せるのは component ファイルから読んだエンジンだけです）。GNOME では入力ソースを切り替えると GNOME Shell が自分の一覧のエンジンに戻すので、試すのは次に切り替えるまでの間です。止めるときは `kill %1` などで終了させると、学習を保存して終わります。

## fcitx5 から IBus に切り替える（Ubuntu 25.10 / GNOME Wayland）

fcitx5 を使っている環境では、fcitx5 が IBus のふりをして（IBus のバスとアドレスファイルを持って）GNOME Shell とやり取りしているため、本物の ibus-daemon と同時には動かせません。どちらか一方に切り替えます。

### IBus にする

1. いまの状態を確かめます。

   ```bash
   im-config -m           # 今の設定（fcitx5 / ibus など）が出る
   pgrep -a fcitx5
   pgrep -a ibus-daemon
   ```

2. 入力メソッドの設定を IBus にします。fcitx5 を自動起動に登録している場合は、それも止めます。

   ```bash
   im-config -n ibus
   ls ~/.config/autostart/ | grep -i fcitx
   # 出てきた場合だけ、名前を変えて退避する
   mv ~/.config/autostart/org.fcitx.Fcitx5.desktop ~/.config/autostart/org.fcitx.Fcitx5.desktop.off
   ```

3. ログアウトしてログインし直します。

4. `pgrep -a fcitx5` が何も出さず、`pgrep -a ibus-daemon` が出せば切り替わっています。`ibus engine` で今のエンジン名が表示されます。

### GNOME の入力ソースに karukan を足す

[インストール](#インストール)を済ませてから、設定 → キーボード →「入力ソース」の「＋ 入力ソースを追加」→「日本語」→「日本語 (Karukan)」を選びます。入力ソースは Super+Space で切り替えます。

コマンドで足すこともできます。今の一覧を確かめてから、`('ibus', 'karukan')` を加えた一覧を書き込みます。

```bash
gsettings get org.gnome.desktop.input-sources sources
# 例: [('xkb', 'us')] だった場合
gsettings set org.gnome.desktop.input-sources sources "[('xkb', 'us'), ('ibus', 'karukan')]"
```

### fcitx5 に戻す

1. 入力ソースから karukan を外します（設定のキーボードの画面で削除するか、`gsettings set` で元の一覧に戻します）。
2. 入力メソッドの設定を fcitx5 に戻し、自動起動を退避していたら元に戻します。

   ```bash
   im-config -n fcitx5
   mv ~/.config/autostart/org.fcitx.Fcitx5.desktop.off ~/.config/autostart/org.fcitx.Fcitx5.desktop
   ```

3. ログアウトしてログインし直し、`pgrep -a fcitx5` が出ることを確かめます。

## ログ

ibus-daemon が起動したエンジンのログは、ibus-daemon と同じ出力先（GNOME ではユーザーのジャーナル）に出ます。手で起動した場合は端末に出ます。量は環境変数 `RUST_LOG`（既定は `info`）で変えられます。

```bash
journalctl --user -f | grep -i karukan
```

## 学習の保存

学習は fcitx5 版と同じく、フォーカスが外れたとき・IBus でほかのエンジンに切り替えたときに保存します。ほかに、IBus がエンジンを破棄したとき、ibus-daemon が終了したとき、プロセスが SIGTERM / SIGINT を受けたときにも保存してから終わります。

## fcitx5 版との違い

- 候補の説明（`[日付]` など）は候補の右の欄ではなく、候補の文字の後ろに続けて出ます。IBus の候補表には説明の欄がないためです。
- 候補表には今のページだけを送ります（ページ分けは karukan がしています）。ページ送りは PageUp / PageDown キーで行います。
- 入力モード（あ / A）は、補助テキスト（候補表の上の行）に出るだけです。パネルのアイコンやメニューにはまだ出ません。
- フォーカスが外れたときに確定される文字は、IBus が表示中の preedit をそのまま確定したものです。fcitx5 版はエンジンが Enter と同じ確定をするので、たとえば末尾にローマ字が残っているとき（`nihon` の `n`）に違いが出ることがあります。学習には Enter と同じ確定の結果が記録されます。
- 変換中に「でばっぐもーど」のコマンド候補を選んだままフォーカスを外すと、IBus がその候補の文字（【…をオン】）を確定してしまいます。
