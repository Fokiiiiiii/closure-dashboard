import { randomBytes, timingSafeEqual } from 'node:crypto';
import { dev } from '$app/env';
import * as env from '$app/env/private';
import type { Cookies } from '@sveltejs/kit';
import { required } from './config';
import { isSnowflake, signSession, SESSION_AGE } from './session';
export const cookieOptions = { path: '/', httpOnly: true, sameSite: 'lax' as const, secure: !dev };
export function stateMatches(expected: string | undefined, actual: string | null): boolean {
	if (!expected || !actual) return false;
	const a = Buffer.from(expected); const b = Buffer.from(actual);
	return a.length === b.length && timingSafeEqual(a, b);
}
export function startOAuth(cookies: Cookies) {
	const state = randomBytes(32).toString('base64url');
	cookies.set('oauth_state', state, { ...cookieOptions, maxAge: 600 });
	const url = new URL('https://discord.com/oauth2/authorize');
	url.search = new URLSearchParams({ client_id: required('DISCORD_CLIENT_ID'), redirect_uri: required('DISCORD_REDIRECT_URI'), response_type: 'code', scope: 'identify', state }).toString();
	return url.toString();
}
export async function finishOAuth(cookies: Cookies, url: URL, fetcher: typeof fetch) {
	const expected = cookies.get('oauth_state');
	cookies.delete('oauth_state', cookieOptions);
	if (!stateMatches(expected, url.searchParams.get('state')) || !url.searchParams.get('code')) return false;
	const response = await fetcher('https://discord.com/api/oauth2/token', { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ client_id: required('DISCORD_CLIENT_ID'), client_secret: required('DISCORD_CLIENT_SECRET'), grant_type: 'authorization_code', code: url.searchParams.get('code')!, redirect_uri: required('DISCORD_REDIRECT_URI') }) });
	if (!response.ok) return false;
	const token = await response.json();
	if (typeof token.access_token !== 'string') return false;
	const userResponse = await fetcher('https://discord.com/api/users/@me', { headers: { Authorization: `Bearer ${token.access_token}` } });
	if (!userResponse.ok) return false;
	const user = await userResponse.json();
	if (typeof user.id !== 'string' || !isSnowflake(user.id)) return false;
	setSession(cookies, user.id);
	return true;
}
export function setSession(cookies: Cookies, id: string) { cookies.set('session', signSession(id, required('SESSION_SECRET')), { ...cookieOptions, maxAge: SESSION_AGE }); }
export function devUser() { return dev && env.DASHBOARD_DEV_USER_ID && isSnowflake(env.DASHBOARD_DEV_USER_ID) ? env.DASHBOARD_DEV_USER_ID : null; }
