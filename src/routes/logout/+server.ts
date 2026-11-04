import { redirect } from '@sveltejs/kit';
import { cookieOptions } from '#lib/server/oauth.js';
export function POST({ cookies }: import('./$types').RequestEvent) { cookies.delete('session', cookieOptions); redirect(303, '/login'); }
