# Aurore Player

Linux 向けの音声プレーヤーです。Rust（iced + rodio）で書かれています。

## 対応形式

- 直接再生: mp3、flac、wav、ogg、m4a、aac
- ffmpeg 経由: wma、wmv

WMA は、ffmpeg を外部プロセスとして呼び出して読み込みます。一時ファイルは作りません。

## 依存パッケージ

Arch Linux の場合:

```bash
sudo pacman -S gtk3 alsa-lib ffmpeg
```

- `gtk3`: ファイル選択ダイアログ（rfd）
- `alsa-lib`: 音声出力（rodio / cpal）
- `ffmpeg`: WMA の読み込み（実行時のみ必要）

Rust のツールチェーンも必要です。

```bash
sudo pacman -S rust
```

## ビルド

```bash
git clone https://github.com/ALBAlinux-project/Aurore-player.git
cd Aurore-player
cargo build --release --locked
```

実行ファイルは `target/release/aurore-player` に作られます。

```bash
./target/release/aurore-player
```

## パッケージ化（makepkg）

同梱の `PKGBUILD` を使うと、Arch のパッケージとして入れられます。

```bash
makepkg -si
```

## ライセンス

GPL-3.0-or-later
