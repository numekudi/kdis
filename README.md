# kdis

GPUIで作った、格闘ゲーム向けのキーディスプレイです。グローバルに押されたキーと押下時間を、新しい順で透過オーバーレイに表示します。

キーを押す・離すたびに現在の同時押し状態を記録し、キーコード順に横並びで表示します。方向キーは`↑`、`←`、`→`、`↓`表記です。

キーが押されていない時間はキー欄が空の行として表示します。時間表示は`9999 ms`が上限です。

## デモ

<video src="assets/demo.webm" controls muted></video>

[デモ動画を開く](assets/demo.webm)

## 起動

```sh
cargo run --release
```

## Windows向けクロスビルド

LinuxにRustのGNUターゲットとMinGW-w64を導入します（Ubuntuの例）。

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64
cargo build --target x86_64-pc-windows-gnu
```

実行ファイルは`target/x86_64-pc-windows-gnu/debug/kdis.exe`に生成されます。Windowsではグローバル入力の取得に低レベルキーボードフックを使い、追加の入力権限は不要です。

releaseビルドも同じターゲットを指定して生成できます。GPUI 0.2.2だけは、Linuxホストで実行されないオフラインFXC処理を避けるためランタイムシェーダー経路を使用します。

```sh
cargo build --release --target x86_64-pc-windows-gnu
```

- 左ドラッグ: ウィンドウを移動
- 右クリック: 終了

通常時はキー名と押下時間だけを描画します。ウィンドウをフォーカスすると、移動・操作位置が分かるように行背景を表示します。

Linuxでは最前面表示を確実にするため、GPUIをX11/XWaylandバックエンドで動作させます。Waylandセッションでも`DISPLAY`が設定された一般的なデスクトップ環境なら、そのまま起動できます。

## Linux / Waylandの入力権限

Waylandでは、グローバルキー入力を読むために`/dev/input/event*`へのアクセス権が必要です。ディストリビューションの標準的な方法で、実行ユーザーに入力デバイスの読み取り権限を付与してください。例えば`input`グループを使う環境では次のように設定し、その後ログインし直します。

```sh
sudo usermod -aG input "$USER"
```

必要な権限がない場合は、オーバーレイ内にエラーを表示します。入力デバイス全体を読める強い権限なので、共有環境では管理者の方針に従ってください。
