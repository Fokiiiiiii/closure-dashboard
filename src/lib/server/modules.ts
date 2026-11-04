import { ClosureError, type Config, type Schema } from './closure';
export function moduleNames(schema: Schema): Record<string, string> {
 const modules = schema.properties?.modules;
 const props = modules?.properties ?? modules?.anyOf?.find(x => x.properties)?.properties ?? {};
 return Object.fromEntries(Object.entries(props).map(([id, value]) => [id, typeof value.title === 'string' ? value.title : id]));
}
export async function saveModule(api: { config: (id: string) => Promise<{ data: Config; etag: string }>; putConfig: (id: string, config: Config, etag: string, actor: string) => Promise<unknown> }, guild: string, actor: string, module: string, enabled: boolean) {
 for (let attempt = 0; attempt < 2; attempt++) {
  const { data, etag } = await api.config(guild);
  const modules = data.modules ?? {};
  const updated = { ...data, modules: { ...modules, [module]: { ...modules[module], enabled } } };
  try { await api.putConfig(guild, updated, etag, actor); return; }
  catch (e) { if (!(e instanceof ClosureError && e.status === 412 && attempt === 0)) throw e; }
 }
}
