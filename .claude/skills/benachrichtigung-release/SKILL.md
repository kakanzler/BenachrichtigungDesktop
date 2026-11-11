---
name: benachrichtigung-release
description: Commit, push, merge and install a Benachrichtigung build. Use when the user says release / リリース / 出して / 再インストール / コミットしてマージ / "commit push merge install", or after finishing a change they want installed. Runs verification, commits on a branch, opens and merges a PR, builds the installer from main, installs it silently and restarts the card.
---

# Benachrichtigung リリース手順

検証 → ブランチにコミット → push → PR → マージ → main からビルド → サイレントインストール → 再起動 までを一息で行う。
このスキルの呼び出しが、これらの操作すべての承認にあたる。改めて確認しない。

**途中で失敗したら止まって報告する。** どこまで進んだかを必ず伝える。半端に進んだ状態のまま先へ進まない。

## 0. 事前チェック

```powershell
git status --short --branch
git fetch origin
git log origin/main..HEAD --oneline
git log HEAD..origin/main --oneline
gh pr list --state open --json number,title,headRefName
(Get-Item 'D:\Program Files\Benachrichtigung\benachrichtigung.exe' -ErrorAction SilentlyContinue).LastWriteTime
git log -1 --format=%cI origin/main
```

- **すでに全部済んでいることがある。** 作業ツリーがクリーンで、HEAD が `origin/main` と同じで、インストール済み exe の時刻が最新コミットより新しければ、「もう出ている」と報告して終わる
- 差分の中身を確認し、**身に覚えのない未追跡ファイルは add しない**。ユーザーのものか確認する
- `origin/main` が先行している、または開いている PR が今回の作業と関係ありそうなら、止まって報告する

## 1. 検証(飛ばさない)

```powershell
./verify.ps1
```

`All checks passed.` を確認してから先へ進む。

動作確認で **実物の English / Forscher を起動しない**。English は起動時に実データを更新する(`CLAUDE.md` 参照)。

## 2. ブランチにコミット

```powershell
git switch -c <type>/<topic>    # main にいるときだけ。作業ブランチにいるならそのまま使う
git add <変更したファイル>
git diff --cached --stat
git commit -m "<何をなぜ変えたか>"
```

- `git add -A` と `git add .` は使わない
- コミット前に `--cached --stat` を見て、`node_modules/`、`target/`、`.env`、DB ファイル、ログが入っていないことを確かめる。入っていたら止まる
- `Cargo.lock` と `frontend/package-lock.json` はコミットする
- 仕様を変えたら `CLAUDE.md` と `README.md` にも反映してから add する
- 両アプリのロジックの写しを変えたら、`CLAUDE.md` の対応表も更新する
- コミットメッセージと PR 本文の末尾には、セッションの指示にある帰属表記を付ける

## 3. push、PR、マージ

```powershell
git push -u origin HEAD
gh pr create --base main --fill    # 本文に要約と検証結果を書く場合は --title と --body を使う
gh pr checks --watch               # チェック未設定のリポジトリでは "no checks" で終わる。失敗ではない
gh pr merge --squash --delete-branch
gh pr view --json state,mergeCommit,url
git switch main
git pull --ff-only origin main
git status --short --branch
```

- `state` が `MERGED` であることを確認する
- **force push、`--admin` でのマージ、フックの迂回はしない。** push 拒否、コンフリクト、チェック失敗はそこで止まって報告する
- 消してよいブランチは、このスキルが今回マージしたものだけ

## 4. インストーラをビルド

main がクリーンで `origin/main` と同じ状態であることを確かめてからビルドする。

```powershell
./build.ps1
```

`cargo build --release` では代用しない。インストーラができない。

## 5. サイレントインストールと再起動

常駐中の exe は上書きできない。カードは保存するデータを持たないので、**インストール先の exe に一致するプロセスだけ**止めてよい。

```powershell
$root  = (git rev-parse --show-toplevel)
$ver   = (Get-Content "$root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
$setup = "$root\src-tauri\target\release\bundle\nsis\Benachrichtigung_${ver}_x64-setup.exe"
$built = "$root\src-tauri\target\release\benachrichtigung.exe"
$dir   = "D:\Program Files\Benachrichtigung"
$t     = Join-Path $dir "benachrichtigung.exe"

Get-Process benachrichtigung -ErrorAction SilentlyContinue | Where-Object Path -eq $t | Stop-Process
Start-Sleep 2
$stray = Get-Process benachrichtigung -ErrorAction SilentlyContinue
if ($stray) { "STILL RUNNING: $($stray.Path)" } else {
  Start-Process $setup -ArgumentList "/S","/D=$dir" -Wait
  Start-Sleep 3
  if (-not (Test-Path $t)) { "MISSING: benachrichtigung.exe" } else {
    $i = Get-Item $t; "OK  benachrichtigung.exe  $($i.LastWriteTime)  $($i.Length) B"
    # Tauri は exe にバンドル種別を埋め込む(ビルド後は UNK、インストーラ内は NSS)ので、その印を除いて比べる
    $norm = { param($p) [Text.Encoding]::Latin1.GetString([IO.File]::ReadAllBytes($p)) -replace 'TYPE_VAR_[A-Z]{3}', 'TYPE_VAR_' }
    if ((& $norm $t) -ceq (& $norm $built)) { "MATCH: same as the build" } else { "STALE: installed exe differs from the one just built" }
    Start-Process $t
    Start-Sleep 5
    "running: $(@(Get-Process benachrichtigung -ErrorAction SilentlyContinue | Where-Object Path -eq $t).Count)"
    "autostart: $((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Benachrichtigung)"
    Get-Content "$env:APPDATA\com.benachrichtigung.desktop\app.log" -Tail 5 -ErrorAction SilentlyContinue
  }
}
```

- **`STILL RUNNING` が出たら止まる。** 別の場所から動いているプロセス(`./dev.ps1` のデバッグ版など)なので、勝手に止めず、ユーザーに閉じてもらう
- **`MATCH` と `running: 1` まで確認する。** 未署名の exe を Defender が消すことがあり、上書き失敗もありうるので、時刻だけでは成功と言えない。`Get-FileHash` の単純比較は埋め込みの印のせいで必ず不一致になる
- **NSIS の `/D=` は最後に置き、値を引用符で囲まない。** 順序を変えると既定の場所に入る
- **`autostart` の値は引用符付きのパスが正常。** 空の場合は、ユーザーがトレイで OFF にした可能性がある。勝手に ON に戻さず、そのまま報告する
- **ユーザーデータはインストールで変わらない。** `config.json` と `autostart-initialized` は `%APPDATA%\com.benachrichtigung.desktop\` にあり、インストーラは触らない

## つまずきポイント

| 症状 | 原因 / 対処 |
|---|---|
| インストール後もカードが古いまま | `STALE` を確認する。起動中で上書きできなかったか、`/D=` の書式で別の場所に入っている |
| `benachrichtigung.exe` が消えた | Defender の誤検知を疑う。除外の追加はシステム設定なので、ユーザー本人が昇格シェルで行う |
| 起動してもカードが出ない | 両方 0 件、または × の直後なら正常。トレイアイコンのツールチップで件数を確認する |
| 件数が `?` | `app.log` を見る。両アプリのスキーマ変更が多い |

## 報告

完了報告には次を必ず含める。

- マージした PR の URL と、main のコミット
- インストール結果(exe の時刻、サイズ、`MATCH` の結果)
- 検証結果(テスト件数)。スキップした項目があれば明示する
