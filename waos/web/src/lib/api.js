export function getToken() {
  return localStorage.getItem('waos_token');
}

export function setToken(token) {
  localStorage.setItem('waos_token', token);
}

export function clearToken() {
  localStorage.removeItem('waos_token');
}

export function isAuthed() {
  return !!getToken();
}

export async function api(path, options = {}) {
  const token = getToken();
  const headers = { 'Content-Type': 'application/json', ...options.headers };
  if (token) headers['Authorization'] = `Bearer ${token}`;

  const res = await fetch(path, { ...options, headers });
  const data = await res.json().catch(() => null);

  if (!res.ok) {
    const msg = data?.error || res.statusText || 'Request failed';
    if (res.status === 401) clearToken();
    throw new Error(msg);
  }
  return data;
}

export function statusColor(s) {
  switch (s) {
    case 'connected': return { bg: 'bg-emerald-500/15', text: 'text-emerald-400', dot: 'bg-emerald-400' };
    case 'disconnected': return { bg: 'bg-red-500/15', text: 'text-red-400', dot: 'bg-red-400' };
    case 'reconnecting': return { bg: 'bg-amber-500/15', text: 'text-amber-400', dot: 'bg-amber-400 animate-pulse-dot' };
    case 'pairing': return { bg: 'bg-blue-500/15', text: 'text-blue-400', dot: 'bg-blue-400 animate-pulse-dot' };
    case 'paired': return { bg: 'bg-emerald-500/15', text: 'text-emerald-400', dot: 'bg-emerald-400' };
    case 'banned': return { bg: 'bg-orange-500/15', text: 'text-orange-400', dot: 'bg-orange-400' };
    default: return { bg: 'bg-gray-500/15', text: 'text-gray-400', dot: 'bg-gray-400' };
  }
}

export function isLoggedIn(status) {
  return status === 'connected' || status === 'paired';
}
