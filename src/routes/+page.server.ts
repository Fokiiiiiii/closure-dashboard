import { redirect, error } from '@sveltejs/kit';
import { closure, ClosureError } from '#lib/server/closure.js';
import { messages } from '#lib/messages.js';
import type { PageServerLoad } from './$types';
export const load: PageServerLoad = async ({ locals, fetch }) => {
 const api = closure(fetch); const m = messages[locals.locale];
 try {
  const guilds = await api.guilds();
  const checks = await Promise.all(guilds.map(async g => ({ guild: g, access: await api.access(g.id, locals.user!.id) })));
  const allowed = checks.filter(x => x.access.allowed).map(x => x.guild);
  if (allowed.length === 1) redirect(303, '/g/' + allowed[0].id);
  return { guilds: allowed };
 } catch (e) { if (e instanceof ClosureError && e.status === 503) error(503, m.unavailable); throw e; }
};
