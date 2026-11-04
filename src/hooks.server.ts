import { error, redirect } from '@sveltejs/kit';
import type { Handle } from '@sveltejs/kit/hooks';
import * as env from '$app/env/private';
import { locale, messages } from '#lib/messages.js';
import { closure, ClosureError } from '#lib/server/closure.js';
import { required } from '#lib/server/config.js';
import { readSession } from '#lib/server/session.js';

export const handle: Handle = async ({ event, resolve }) => {
	const missing = (['CLOSURE_API_URL', 'CLOSURE_ADMIN_TOKEN', 'SESSION_SECRET', 'DISCORD_CLIENT_ID', 'DISCORD_CLIENT_SECRET', 'DISCORD_REDIRECT_URI'] as const).filter(name => !env[name]);
	if (missing.length) return new Response('Missing required environment variables: ' + missing.join(', '), { status: 500, headers: { 'Cache-Control': 'no-store', 'Content-Type': 'text/plain; charset=utf-8' } });
	event.locals.locale = locale(event.request.headers.get('accept-language'));
	const m = messages[event.locals.locale];
	event.locals.user = readSession(event.cookies.get('session'), required('SESSION_SECRET'));
	const path = event.url.pathname;
	if (!event.locals.user && path !== '/login' && !path.startsWith('/auth/')) redirect(303, '/login');
	const match = path.match(/^\/g\/([^/]+)(?:\/|$)/);
	if (match && event.locals.user) {
		try {
			const access = await closure(event.fetch).access(match[1], event.locals.user.id);
			if (!access.allowed) error(403, m.denied);
		} catch (e) {
			if (e instanceof ClosureError) {
				if (e.status === 503) error(503, m.unavailable);
				if (e.status === 404) error(404, m.notFound);
				if (e.status === 403) error(403, m.denied);
			}
			throw e;
		}
	}
	const response = await resolve(event, { transformPageChunk: ({ html }) => html.replace('%lang%', event.locals.locale) });
	response.headers.set('Cache-Control', 'no-store');
	return response;
};
