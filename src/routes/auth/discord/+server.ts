import { redirect } from '@sveltejs/kit';
import { startOAuth } from '#lib/server/oauth.js';
export function GET({ cookies }: import('./$types').RequestEvent) { redirect(302, startOAuth(cookies), { external: ['https://discord.com'] }); }
