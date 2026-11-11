# BenachrichtigungDesktop

English と Forscher の、今日の ActiveRecall 残数をデスクトップ右下に小さく常時表示するアプリです。

- 行をクリックすると対応するアプリが開きます。起動済みなら既存のウィンドウが前面に出ます
- 件数が 0 のアプリの行は表示されません。両方 0 件ならカード全体が消えます
- × でカードを隠せます。いずれかの件数が変わると再表示されます
- 件数は 30 秒ごとに更新されます
- 表示が `?` のときは件数を読み取れていません。原因は `%APPDATA%\com.benachrichtigung.desktop\app.log` にあります
- PC へのサインイン時に自動で起動します
- 見出し「今日の復習」の右にある「Zukunft」ボタンで Zukunft を開きます。Zukunft は件数を持たないので行はありません

タスクトレイのアイコンから、再表示、今すぐ更新、Zukunft を開く、自動起動の ON/OFF、終了を操作できます。カードが隠れているときは、Zukunft はトレイから開けます。

## ビルドとインストール

```powershell
cd frontend; npm install; cd ..
./build.ps1
& "src-tauri\target\release\bundle\nsis\Benachrichtigung_0.1.0_x64-setup.exe" /S "/D=D:\Program Files\Benachrichtigung"
```

インストール後に一度起動すると、自動起動が登録されます。

## 設定

既定のパスと異なる場合だけ、`%APPDATA%\com.benachrichtigung.desktop\config.json` で上書きします。指定は必要な項目だけで構いません。

```json
{
  "englishDb": "C:\\path\\to\\coach.db",
  "englishExe": "C:\\path\\to\\app.exe",
  "forscherDb": "C:\\path\\to\\forscher.db",
  "forscherExe": "C:\\path\\to\\forscher.exe",
  "zukunftExe": "C:\\path\\to\\zukunft.exe"
}
```

開発については [CLAUDE.md](CLAUDE.md) を参照してください。
