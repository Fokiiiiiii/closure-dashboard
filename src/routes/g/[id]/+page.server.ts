import { error, fail, redirect } from '@sveltejs/kit';
import { closure, ClosureError } from '#lib/server/closure.js';
import { moduleNames, saveModule } from '#lib/server/modules.js';
import { messages } from '#lib/messages.js';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ params, fetch, locals }) => {
 const api = closure(fetch); const m = messages[locals.locale];
 try {
  const [guilds, effective, history, schema] = await Promise.all([api.guilds(), api.effective(params.id), api.history(params.id), api.schema(locals.locale)]);
  const names = moduleNames(schema);
  const modules = Object.entries(names).map(([id, name]) => ({ id, name, enabled: effective.modules?.[id]?.enabled === true }));
  return { guild: guilds.find(g => g.id === params.id) ?? { id: params.id, name: params.id, icon: null }, modules, history };
 } catch (e) { if (e instanceof ClosureError && e.status === 503) error(503, m.unavailable); if (e instanceof ClosureError && e.status === 404) error(404, m.notFound); throw e; }
};
export const actions: Actions = { toggle: async ({ request, params, locals, fetch }) => {
 const data = await request.formData(); const id = data.get('module'); const enabled = data.get('enabled'); const m = messages[locals.locale];
 if (typeof id !== 'string' || !/^[a-zA-Z0-9_-]+$/.test(id) || (enabled !== 'true' && enabled !== 'false')) return fail(400, { message: m.invalid });
 const api = closure(fetch);
 try {
  const names = moduleNames(await api.schema(locals.locale));
  if (!Object.hasOwn(names, id)) return fail(400, { message: m.invalid });
  await saveModule(api, params.id, locals.user!.id, id, enabled === 'true');
 } catch (e) {
  if (e instanceof ClosureError && e.status === 422) return fail(422, { message: [e.message, ...e.issues.map(x => x.message)].join(': ') });
  if (e instanceof ClosureError && e.status === 403) return fail(403, { message: e.message });
  if (e instanceof ClosureError && e.status === 412) return fail(412, { message: m.conflict });
  if (e instanceof ClosureError && e.status === 503) return fail(503, { message: m.unavailable });
  throw e;
 }
 redirect(303, '/g/' + params.id);
} };
