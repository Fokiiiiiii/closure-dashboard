import { defineEnvVars } from '@sveltejs/kit/env';
const optional = (value: string | undefined) => value;
export const variables = defineEnvVars({
 CLOSURE_API_URL: { schema: optional }, CLOSURE_ADMIN_TOKEN: { schema: optional },
 SESSION_SECRET: { schema: optional }, DISCORD_CLIENT_ID: { schema: optional },
 DISCORD_CLIENT_SECRET: { schema: optional }, DISCORD_REDIRECT_URI: { schema: optional },
 DASHBOARD_DEV_USER_ID: { schema: optional }
});
