export const adminHtml = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Coucou Control</title><style>
:root{color-scheme:dark;font-family:system-ui,sans-serif;background:#101114;color:#eee}body{max-width:980px;margin:0 auto;padding:24px}h1{margin-bottom:4px}.muted{color:#9ca3af}.card{background:#191b20;border:1px solid #30343b;border-radius:10px;padding:16px;margin:16px 0}label{display:block;margin:8px 0}input,select,button{font:inherit;padding:8px;border-radius:6px;border:1px solid #444;background:#111318;color:#eee}input{min-width:260px}button{cursor:pointer;background:#3159d8;border-color:#3159d8}button.danger{background:#8b2635;border-color:#8b2635}table{width:100%;border-collapse:collapse}th,td{text-align:left;padding:8px;border-bottom:1px solid #30343b}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#0d0e11;padding:12px;border-radius:6px}.error{color:#ff8b8b}.ok{color:#7ee787}</style></head>
<body><h1>Coucou Control</h1><p class="muted">The admin token stays in this tab's memory and is never persisted.</p>
<section class="card" id="login"><form id="login-form"><label>Admin token <input id="token" type="password" autocomplete="off" required></label><button>Connect</button></form></section>
<main id="panel" hidden>
<section class="card"><h2>Approve pairing</h2><form id="pin-form"><input id="pin" inputmode="numeric" pattern="[0-9]{6}" maxlength="6" placeholder="6-digit PIN" required> <button>Approve</button></form></section>
<section class="card"><h2>Devices</h2><button id="refresh">Refresh</button><div id="devices"></div></section>
<section class="card"><h2>Run diagnostic</h2><form id="task-form"><label>Device <select id="task-device" required></select></label><label>Capability <select id="capability"><option>health</option><option>system_info</option></select></label><button>Submit task</button></form></section>
<section class="card"><h2>Task result</h2><form id="result-form"><input id="task-id" placeholder="Task ID" required> <button>View / refresh</button></form><pre id="result">No task selected.</pre></section>
</main><p id="notice" role="status"></p><script src="/control/admin.js" defer></script></body></html>`;

export const adminJs = `(() => {
'use strict';
let token = '';
let currentDevices = [];
const $ = id => document.getElementById(id);
const notice = (message, error = false) => { $('notice').textContent = message; $('notice').className = error ? 'error' : 'ok'; };
async function api(path, options = {}) {
  const headers = {...(options.headers || {}), authorization: 'Bearer ' + token};
  if (options.body) headers['content-type'] = 'application/json';
  const response = await fetch('/control' + path, {...options, headers, cache: 'no-store'});
  const body = await response.json();
  if (!response.ok) throw new Error(body.message || 'Request failed (' + response.status + ')');
  return body;
}
const escapeHtml = value => String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
function loadCapabilities() {
  const device = currentDevices.find(item => item.id === $('task-device').value);
  $('capability').innerHTML = (device?.capabilities || []).map(capability => '<option value="' + escapeHtml(capability) + '">' + escapeHtml(capability) + '</option>').join('');
}
async function loadDevices() {
  const devices = await api('/v1/admin/devices');
  currentDevices = devices;
  $('devices').innerHTML = devices.length ? '<table><thead><tr><th>Name</th><th>Status</th><th>Last seen</th><th>Actions</th></tr></thead><tbody>' + devices.map(device => '<tr><td>' + escapeHtml(device.name) + '</td><td>' + (device.revokedAt ? 'revoked' : device.online ? 'online' : 'offline') + '</td><td>' + escapeHtml(device.lastSeen || 'never') + '</td><td><button data-rename="' + device.id + '">Rename</button> <button class="danger" data-revoke="' + device.id + '"' + (device.revokedAt ? ' disabled' : '') + '>Revoke</button></td></tr>').join('') + '</tbody></table>' : '<p>No devices paired.</p>';
  $('task-device').innerHTML = devices.filter(device => !device.revokedAt).map(device => '<option value="' + device.id + '">' + escapeHtml(device.name) + '</option>').join('');
  loadCapabilities();
}
$('login-form').addEventListener('submit', async event => { event.preventDefault(); token = $('token').value; $('token').value = ''; try { await loadDevices(); $('login').hidden = true; $('panel').hidden = false; notice('Connected.'); } catch (error) { token = ''; notice(error.message, true); } });
$('pin-form').addEventListener('submit', async event => { event.preventDefault(); try { await api('/v1/admin/pairings/approve', {method:'POST', body:JSON.stringify({pin:$('pin').value})}); $('pin').value=''; notice('Pairing approved.'); } catch(error){ notice(error.message,true); } });
$('refresh').addEventListener('click', () => loadDevices().catch(error => notice(error.message,true)));
$('task-device').addEventListener('change', loadCapabilities);
$('devices').addEventListener('click', async event => { const target=event.target; const rename=target.dataset.rename; const revoke=target.dataset.revoke; let action=''; try { if(rename){ const name=prompt('New device name'); if(name === null) return; await api('/v1/admin/devices/'+encodeURIComponent(rename)+'/rename',{method:'POST',body:JSON.stringify({name})}); action='rename'; } else if(revoke){ if(!confirm('Revoke this device?')) return; await api('/v1/admin/devices/'+encodeURIComponent(revoke)+'/revoke',{method:'POST'}); action='revoke'; } if(!action) return; await loadDevices(); notice(action === 'rename' ? 'Device renamed.' : 'Device revoked.'); } catch(error){ notice(error.message,true); } });
$('task-form').addEventListener('submit', async event => { event.preventDefault(); const capability=$('capability').value; try { const task=await api('/v1/admin/tasks',{method:'POST',body:JSON.stringify({deviceId:$('task-device').value,capability,args:{},idempotencyKey:crypto.randomUUID(),deadlineAt:new Date(Date.now()+60000).toISOString(),timeoutMs:30000,approvalClass:'safe',summary:'Admin '+capability+' diagnostic',maxOutputBytes:65536,artifactPolicy:'none'})}); $('task-id').value=task.id; $('result').textContent=JSON.stringify(task,null,2); notice('Task submitted.'); } catch(error){ notice(error.message,true); } });
$('result-form').addEventListener('submit', async event => { event.preventDefault(); try { const task=await api('/v1/admin/tasks/'+encodeURIComponent($('task-id').value)); $('result').textContent=JSON.stringify(task,null,2); notice('Task refreshed.'); } catch(error){ notice(error.message,true); } });
})();`;
