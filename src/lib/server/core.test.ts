import { describe, it, expect, vi, beforeEach } from 'vitest';
vi.mock('$app/env/private', () => ({ CLOSURE_API_URL: 'http://bot.test', CLOSURE_ADMIN_TOKEN: 'secret-token', SESSION_SECRET: 'session-secret', DISCORD_CLIENT_ID:'client', DISCORD_CLIENT_SECRET:'client-secret', DISCORD_REDIRECT_URI:'http://localhost/auth/callback', DASHBOARD_DEV_USER_ID:'123' }));
vi.mock('$app/env', () => ({ dev: false }));
import { signSession, readSession, SESSION_AGE } from './session';
import { closure, ClosureError } from './closure';
import { saveModule, moduleNames } from './modules';
import { finishOAuth, devUser } from './oauth';
import { handle } from '../../hooks.server';
import { actions } from '../../routes/g/[id]/+page.server';
import { GET as start } from '../../routes/auth/discord/+server';

const json = (body: unknown, status = 200, headers: Record<string,string> = {}) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json', ...headers } });
describe('session', () => {
 it('signs, verifies, rejects tampering and expiry', () => {
  const cookie = signSession('123', 'key', 1000);
  expect(readSession(cookie, 'key', 1000)).toEqual({ id: '123' });
  expect(readSession(cookie + 'a', 'key', 1000)).toBeNull();
  expect(readSession(cookie, 'wrong', 1000)).toBeNull();
  expect(readSession(cookie, 'key', 1000 + SESSION_AGE * 1000)).toBeNull();
 });
});
describe('closure client', () => {
 it('sends bearer, ETag and actor', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(json({}, 200, { etag:'"1"' })).mockResolvedValueOnce(json({ revision:2 }));
  const api = closure(fetcher);
  const stored = await api.config('42');
  await api.putConfig('42', stored.data, stored.etag, '123');
  expect(fetcher.mock.calls[0][1].headers.Authorization).toBe('Bearer secret-token');
  expect(fetcher.mock.calls[1][1].headers).toMatchObject({ Authorization:'Bearer secret-token', 'If-Match':'"1"', 'X-Closure-Actor':'123' });
 });
 it.each([401,403,404,412,422,428,503,429,413])('classifies %i and removes secret', async status => {
  const api = closure(vi.fn().mockResolvedValue(json({error:{code:'error', message:'secret-token',issues:[{path:'x',code:'x',message:'secret-token'}]}},status)));
  try { await api.guilds(); throw new Error('expected failure'); } catch(e) {
   expect(e).toBeInstanceOf(ClosureError); expect((e as ClosureError).status).toBe(status);
   expect(JSON.stringify(e)).not.toContain('secret-token'); expect((e as Error).message).not.toContain('secret-token');
  }
 });
 it('does not return the bearer token from a response, even with JSON escapes', async () => {
  const api = closure(vi.fn().mockResolvedValue(new Response('[{"id":"1","name":"secret-\\u0074oken","icon":null}]')));
  expect(JSON.stringify(await api.guilds())).not.toContain('secret-token');
 });
});
describe('oauth', () => {
 it('redirects to the Discord authorization endpoint', () => {
  const cookies = { set: vi.fn() };
  expect(() => start({ cookies } as never)).toThrow(expect.objectContaining({ status: 302, location: expect.stringContaining('https://discord.com/oauth2/authorize?') }));
  expect(cookies.set).toHaveBeenCalled();
 });
 it('rejects state mismatch without exchanging a token', async () => {
  const fetcher = vi.fn(); const cookies = { get:()=> 'expected', delete:vi.fn() };
  expect(await finishOAuth(cookies as never, new URL('http://localhost/?state=wrong&code=x'), fetcher)).toBe(false);
  expect(fetcher).not.toHaveBeenCalled(); expect(cookies.delete).toHaveBeenCalled();
 });
 it('disables development login in production', () => expect(devUser()).toBeNull());
});
describe('hook', () => {
 function event(path: string, cookie?: string, fetcher = vi.fn()) {
  return { url:new URL('http://localhost' + path), request:new Request('http://localhost' + path), cookies:{get:()=>cookie}, locals:{}, fetch:fetcher };
 }
 it('redirects anonymous users', async () => {
  await expect(handle({event:event('/') as never, resolve:vi.fn()})).rejects.toMatchObject({status:303,location:'/login'});
 });
 it.each([[403, json({allowed:false})],[503,json({error:{code:'unavailable',message:'unavailable'}},503)],[404,json({error:{code:'not_found',message:'not found'}},404)]])('rejects guild access with %i', async (status, response) => {
  const fetcher = vi.fn().mockResolvedValue(response);
  await expect(handle({event:event('/g/42',signSession('123','session-secret'),fetcher) as never,resolve:vi.fn()})).rejects.toMatchObject({status});
  expect(fetcher).toHaveBeenCalledTimes(1);
 });
});
describe('module save', () => {
 it('extracts modules from nullable schema', () => expect(moduleNames({properties:{modules:{anyOf:[{properties:{mod:{title:'Moderation'}}}]}}})).toEqual({mod:'Moderation'}));
 it('refetches once after 412 and preserves the latest document', async () => {
  const api = {config:vi.fn().mockResolvedValueOnce({data:{preset:'small'},etag:'"1"'}).mockResolvedValueOnce({data:{preset:'large',modules:{mod:{config:{x:1}}}},etag:'"2"'}),putConfig:vi.fn().mockRejectedValueOnce(new ClosureError(412,'conflict','conflict')).mockResolvedValueOnce({revision:3})};
  await saveModule(api,'42','123','mod',true);
  expect(api.config).toHaveBeenCalledTimes(2);
  expect(api.putConfig).toHaveBeenLastCalledWith('42',{preset:'large',modules:{mod:{config:{x:1},enabled:true}}},'"2"','123');
 });
 it('reports 422 validation details from the action', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(json({properties:{modules:{anyOf:[{properties:{mod:{}}}]}}})).mockResolvedValueOnce(json({},200,{etag:'"1"'})).mockResolvedValueOnce(json({error:{code:'invalid_config',message:'Invalid',issues:[{path:'mod',code:'bad',message:'Bad value'}]}},422));
  const form = new FormData(); form.set('module','mod'); form.set('enabled','true');
  const result = await actions.toggle!({ request:new Request('http://localhost/g/42',{method:'POST',body:form}), params:{id:'42'}, locals:{user:{id:'123'},locale:'en'}, fetch:fetcher } as never);
  expect(result).toMatchObject({ status:422, data:{message:'Invalid: Bad value'} });
 });
});
