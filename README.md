# closure dashboard

環境変数は `.env.example` を参照し、実行環境で設定してください。Discord Developer Portal の OAuth2 redirect に `DISCORD_REDIRECT_URI` を登録します。Bot 側で HTTP API を有効にし、管理トークンを dashboard の `CLOSURE_ADMIN_TOKEN` と一致させてください。

```sh
npm run dev
npm run check
npm test
npm run build
node build
```

`DASHBOARD_DEV_USER_ID` は開発サーバー専用です。本番ビルドでは無視されます。公開運用では HTTPS を使用してください。adapter-node は標準で HTTPS として URL を判定します。信頼できるリバースプロキシを使う場合は、プロキシが接続元のヘッダーを上書きする設定にしたうえで、必要に応じて `PROTOCOL_HEADER` など adapter-node のヘッダー設定を指定してください。
