import { error, redirect } from '@sveltejs/kit';
import { finishOAuth } from '#lib/server/oauth.js';
import { messages } from '#lib/messages.js';
export async function GET({ cookies, url, fetch, locals }: import('./$types').RequestEvent) {
 if (!await finishOAuth(cookies, url, fetch)) error(400, messages[locals.locale].oauthFailed);
 redirect(303, '/');
}
