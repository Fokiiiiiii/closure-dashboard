<script lang="ts">
 import { messages } from '#lib/messages.js';
 let { data, form } = $props();
 let m = $derived(messages[data.locale]);
 let active = $derived(data.modules.filter((x: { enabled: boolean }) => x.enabled).length);
 function date(timestamp: number) { return new Intl.DateTimeFormat(data.locale, { dateStyle: 'medium', timeStyle: 'short' }).format(timestamp * 1000); }
</script>
<svelte:head><title>{m.overview} · {data.guild.name}</title></svelte:head>
<div class="shell">
 <aside><a class="server-select" href="/">{#if data.guild.icon}<img class="avatar" src={'https://cdn.discordapp.com/icons/' + data.guild.id + '/' + data.guild.icon + '.png'} alt="" />{:else}<span class="avatar">{data.guild.name.slice(0,1)}</span>{/if}<span>{data.guild.name}</span><span aria-hidden="true">⌄</span></a>
 <nav aria-label={m.title}><a class="nav-active" href={'/g/' + data.guild.id}>⌂ &nbsp; {m.overview}</a><a href="#modules">▦ &nbsp; {m.modules}</a></nav>
 <div class="aside-bottom"><span>{data.user?.id}</span><button class="secondary" type="button" aria-label={m.theme} onclick={() => { const current = document.documentElement.dataset.theme || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'); const next = current === 'dark' ? 'light' : 'dark'; document.documentElement.dataset.theme = next; document.cookie = 'theme=' + next + '; Path=/; Max-Age=31536000; SameSite=Lax'; }}>◐</button><form method="POST" action="/logout"><button class="secondary">{m.logout}</button></form></div></aside>
 <div class="workspace"><div class="breadcrumb"><a href="/">{m.servers}</a> / {data.guild.name} / {m.overview}</div>
 <main><h1>{m.overview}</h1><section class="stats" aria-label={m.status}><div class="card stat"><span>{m.bot}</span><strong><i class="dot"></i>{m.connected}</strong></div><div class="card stat"><span>{m.activeModules}</span><strong>{active} <small>/ {data.modules.length}</small></strong></div><div class="card stat"><span>{m.lastChange}</span><strong class="stat-date">{data.history[0] ? date(data.history[0].created_at_unix) : '—'}</strong></div></section>
 {#if form?.message}<div class="alert" role="alert">{form.message}</div>{/if}
 <section class="card" id="modules"><h2>{m.modules}</h2><table><thead><tr><th>{m.name}</th><th>{m.status}</th><th><span class="sr-only">{m.change}</span></th></tr></thead><tbody>{#each data.modules as module}<tr><td>{module.name}</td><td><span class:badge-on={module.enabled} class="badge">{module.enabled ? m.enabled : m.disabled}</span></td><td><form method="POST" action="?/toggle"><input type="hidden" name="module" value={module.id}/><input type="hidden" name="enabled" value={String(!module.enabled)}/><button class="switch" class:on={module.enabled} role="switch" aria-checked={module.enabled} aria-label={m.change + ' ' + module.name}><span></span></button></form></td></tr>{/each}</tbody></table>{#if !data.modules.length}<p class="empty">{m.noModules}</p>{/if}</section>
 <section class="card"><h2>{m.recent}</h2>{#if data.history.length}<ul class="history">{#each data.history as item}<li><span>{item.actor ?? m.unknownActor}</span><time datetime={new Date(item.created_at_unix * 1000).toISOString()}>{date(item.created_at_unix)}</time></li>{/each}</ul>{:else}<p class="empty">{m.none}</p>{/if}</section>
 </main></div>
</div>
