<script lang="ts">
 import { messages } from '#lib/messages.js';
 let { data } = $props();
 let m = $derived(messages[data.locale]);
</script>
<svelte:head><title>{m.servers} · closure</title></svelte:head>
<div class="shell"><aside><div class="brand">closure</div><form method="POST" action="/logout"><button class="secondary">{m.logout}</button></form></aside><div class="workspace"><div class="breadcrumb">{m.title}</div><main><h1>{m.servers}</h1>{#if data.guilds.length}<div class="card">{#each data.guilds as guild}<a class="server-row" href={'/g/' + guild.id}>{#if guild.icon}<img class="avatar" src={'https://cdn.discordapp.com/icons/' + guild.id + '/' + guild.icon + '.png'} alt="" />{:else}<span class="avatar">{guild.name.slice(0,1)}</span>{/if}<span>{guild.name}</span><span aria-hidden="true">›</span></a>{/each}</div>{:else}<p class="muted">{m.empty}</p>{/if}</main></div></div>
