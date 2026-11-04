import { error, redirect } from '@sveltejs/kit';
import { devUser, setSession } from '#lib/server/oauth.js';
import type { Actions, PageServerLoad } from './$types';
export const load: PageServerLoad = () => ({ devLogin: !!devUser() });
export const actions: Actions = { dev: ({ cookies }) => { const id = devUser(); if (!id) error(404); setSession(cookies, id); redirect(303, '/'); } };
