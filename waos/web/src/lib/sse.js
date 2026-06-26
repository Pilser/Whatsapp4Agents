const MAX_EVENTS = 300;
let eventLog = [];
let listeners = new Map();
let es = null;

export function getEventLog() {
  return eventLog;
}

export function onEvent(type, fn) {
  if (!listeners.has(type)) listeners.set(type, []);
  listeners.get(type).push(fn);
  return () => {
    const arr = listeners.get(type);
    if (arr) listeners.set(type, arr.filter(f => f !== fn));
  };
}

export function connect() {
  if (es) es.close();
  es = new EventSource('/events');

  es.onmessage = (msg) => {
    try {
      const data = JSON.parse(msg.data);
      const type = Object.keys(data)[0];
      const payload = data[type];
      if (!payload) return;

      const entry = { time: Date.now(), type, ...payload };
      eventLog.push(entry);
      if (eventLog.length > MAX_EVENTS) eventLog.shift();

      const cbs = listeners.get(type);
      if (cbs) cbs.forEach(fn => fn(payload, entry));

      const wildcard = listeners.get('*');
      if (wildcard) wildcard.forEach(fn => fn(payload, entry));
    } catch {}
  };

  es.onerror = () => {
    setTimeout(connect, 3000);
  };
}

export function disconnect() {
  if (es) { es.close(); es = null; }
}
