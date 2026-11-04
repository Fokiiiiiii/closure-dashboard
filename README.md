# closure-dashboard

Rust 製の closure 管理画面です。環境変数を設定して起動します（`.env` の自動読み込みはありません）。

`CLOSURE_API_URL`、`CLOSURE_ADMIN_TOKEN`、`SESSION_SECRET`（32 文字以上）、`DISCORD_CLIENT_ID`、`DISCORD_CLIENT_SECRET`、`DISCORD_REDIRECT_URI` が必須です。Discord 側に同じ callback URL を登録してください。Bot API は管理トークンを受け付ける必要があります。`DASHBOARD_BIND` の既定値は `127.0.0.1:3000` です。

```sh
cargo run --release
```

ブラウザで接続し、Discord の identify 権限でログインします。開発ビルドでは `DASHBOARD_DEV_USER_ID` を指定すると開発用ログインを利用できます。本番ビルドでは無効です。

実 Bot に接続するには、closure の管理 API を起動して `CLOSURE_API_URL` にその URL、`CLOSURE_ADMIN_TOKEN` に Bot 側で設定した管理トークンを指定します。Discord Developer Portal にアプリを登録し、`DISCORD_CLIENT_ID`、`DISCORD_CLIENT_SECRET`、登録した callback URL と一致する `DISCORD_REDIRECT_URI` を設定してください。推測されない `SESSION_SECRET` を設定してダッシュボードを起動し、Discord でログインします。公開環境では HTTPS を使用してください。

環境変数は上記の 6 個に加え、待受アドレスを指定する `DASHBOARD_BIND` と、開発ビルドでのみ有効な Discord ユーザー ID の `DASHBOARD_DEV_USER_ID` です。

翻訳を追加するには、`locales/xx.ftl` を作り、`src/i18n.rs` のレジストリにタグ、自称、文字方向、`include_str!` を 1 行追加します。`cargo test --offline` で翻訳の漏れや変数名の不一致を検査します。

言語は登録済みの `lang` Cookie、次に `Accept-Language`、最後に日本語の順で決まります。Bot API に渡す言語は日本語なら `ja`、それ以外は `en` です。
