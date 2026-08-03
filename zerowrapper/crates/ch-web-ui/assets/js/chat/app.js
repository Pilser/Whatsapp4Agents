// network.js
var MAX_RECONNECT_DELAY = 30000;
var BASE_RECONNECT_DELAY = 1000;
var State = {
  DISCONNECTED: "disconnected",
  CONNECTING: "connecting",
  CONNECTED: "connected",
  RECONNECTING: "reconnecting"
};
var ws = null;
var state = State.DISCONNECTED;
var url = "";
var callbacks = { onOpen: null, onMessage: null, onClose: null };
var reconnectTimer = null;
var reconnectAttempts = 0;
var listenerMap = new Map;
function getState() {
  return state;
}
function on(event, fn) {
  if (!listenerMap.has(event))
    listenerMap.set(event, new Set);
  listenerMap.get(event).add(fn);
  return () => listenerMap.get(event)?.delete(fn);
}
function emit(event, data) {
  listenerMap.get(event)?.forEach((fn) => fn(data));
}
function connect(wsUrl, handlers) {
  if (wsUrl)
    url = wsUrl;
  if (handlers)
    callbacks = handlers;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING))
    return;
  setState(State.CONNECTING);
  ws = new WebSocket(url);
  ws.onopen = onWsOpen;
  ws.onmessage = onWsMessage;
  ws.onclose = onWsClose;
  ws.onerror = onWsError;
}
function disconnect() {
  stopReconnect();
  reconnectAttempts = 0;
  if (ws) {
    ws.onclose = null;
    ws.onerror = null;
    ws.close();
    ws = null;
  }
  setState(State.DISCONNECTED);
}
function send(data) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(typeof data === "string" ? data : JSON.stringify(data));
  }
}
function sendCancel(sessionId) {
  send(JSON.stringify({ type: "cancel", session_id: sessionId || "" }));
}
function setState(s) {
  state = s;
  emit("stateChange", s);
}
function onWsOpen() {
  setState(State.CONNECTED);
  reconnectAttempts = 0;
  if (callbacks.onOpen)
    callbacks.onOpen();
}
function onWsMessage(e) {
  if (callbacks.onMessage)
    callbacks.onMessage(e);
}
function onWsClose(e) {
  setState(State.DISCONNECTED);
  if (callbacks.onClose)
    callbacks.onClose(e);
  scheduleReconnect();
}
function onWsError() {}
function scheduleReconnect() {
  const delay = Math.min(BASE_RECONNECT_DELAY * Math.pow(2, reconnectAttempts), MAX_RECONNECT_DELAY);
  reconnectAttempts++;
  emit("reconnecting", { attempt: reconnectAttempts, delay });
  setState(State.RECONNECTING);
  reconnectTimer = setTimeout(() => {
    if (url)
      connect();
  }, delay);
}
function stopReconnect() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}
window.addEventListener("online", () => {
  if (state === State.RECONNECTING || state === State.DISCONNECTED) {
    stopReconnect();
    if (url)
      connect();
  }
});
window.addEventListener("offline", () => {
  emit("offline");
});

// dom.js
var els = {};
function init() {
  els.form = document.getElementById("chat-form");
  els.input = document.getElementById("chat-input");
  els.messages = document.getElementById("chat-messages");
  els.hint = document.getElementById("fallback-hint");
  els.hintText = document.getElementById("chat-hint-text");
  els.chatRefreshBtn = document.getElementById("chat-refresh-btn");
  els.typingEl = document.getElementById("typing-indicator");
  els.authSection = document.getElementById("auth-section");
  els.userBadge = document.getElementById("user-badge");
  els.badgeDisplay = document.getElementById("badge-display");
  els.registerBtn = document.getElementById("register-btn");
  els.phoneInput = document.getElementById("phone-input");
  els.nameInput = document.getElementById("name-input");
  els.countryCode = document.getElementById("country-code");
  els.authError = document.getElementById("auth-error");
  els.logoutBtn = document.getElementById("logout-btn");
  els.exampleChips = document.getElementById("example-chips");
  els.sendBtn = document.getElementById("send-btn");
  els.cancelBtn = document.getElementById("cancel-btn");
  els.ownerBtn = document.getElementById("owner-btn");
  els.ownerBtnMobile = document.getElementById("owner-btn-mobile");
  els.ownerLoginOverlay = document.getElementById("owner-login-overlay");
  els.ownerChat = document.getElementById("owner-chat");
  els.ownerRoleBadge = document.getElementById("owner-role-badge");
  els.ownerBackBtn = document.getElementById("owner-back-btn");
}

// auth.js
var STORAGE_PHONE = "zerowrapper_phone";
var STORAGE_NAME = "zerowrapper_name";
var STORAGE_SESSION = "zerowrapper_session";
var state2 = {
  phone: "",
  displayName: "",
  sessionId: ""
};
function loadFromStorage() {
  state2.phone = localStorage.getItem(STORAGE_PHONE) || "";
  state2.displayName = localStorage.getItem(STORAGE_NAME) || "";
  state2.sessionId = localStorage.getItem(STORAGE_SESSION) || "";
}
function saveToStorage() {
  if (state2.phone)
    localStorage.setItem(STORAGE_PHONE, state2.phone);
  if (state2.displayName)
    localStorage.setItem(STORAGE_NAME, state2.displayName);
  if (state2.sessionId)
    localStorage.setItem(STORAGE_SESSION, state2.sessionId);
}
function clearFromStorage() {
  localStorage.removeItem(STORAGE_PHONE);
  localStorage.removeItem(STORAGE_NAME);
  localStorage.removeItem(STORAGE_SESSION);
}
function normalizePhone(code, num) {
  num = num.replace(/^0+/, "");
  code = code.replace(/^\+/, "");
  return code + num;
}
function showForm() {
  els.authSection.classList.remove("hidden");
  els.userBadge.classList.add("hidden");
}
function showBadge() {
  els.authSection.classList.add("hidden");
  els.userBadge.classList.remove("hidden");
  const name = state2.displayName || state2.phone;
  els.badgeDisplay.textContent = state2.phone + (name && name !== state2.phone ? " (" + name + ")" : "");
}
function scrollToAuth() {
  els.authSection.scrollIntoView({ behavior: "smooth", block: "center" });
  els.phoneInput.focus();
  els.authSection.style.transition = "box-shadow 0.3s";
  els.authSection.style.boxShadow = "0 0 0 3px var(--color-bg-interactive)";
  setTimeout(() => {
    els.authSection.style.boxShadow = "none";
  }, 2000);
}
function showAuthError(msg) {
  els.authError.textContent = msg;
  els.authError.classList.remove("hidden");
}
function hideAuthError() {
  els.authError.classList.add("hidden");
}

// messages.js
import { marked as marked2 } from "marked";

// constants.js
var USER_AVATAR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>';
var BOT_AVATAR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>';
function escapeHtml(str) {
  if (!str)
    return "";
  return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

// markdown.js
import hljs from "highlight.js";
function highlight(el) {
  el.querySelectorAll("pre code").forEach((block) => {
    hljs.highlightElement(block);
  });
}
function wrapTables(el) {
  el.querySelectorAll("table").forEach((t) => {
    if (t.parentElement && t.parentElement.classList.contains("table-wrap"))
      return;
    const outer = document.createElement("div");
    outer.className = "table-outer-wrap";
    const wrapper = document.createElement("div");
    wrapper.className = "table-wrap";
    t.parentNode.insertBefore(outer, t);
    outer.appendChild(wrapper);
    wrapper.appendChild(t);
    const fade = document.createElement("div");
    fade.className = "scroll-fade";
    outer.appendChild(fade);
    const arrow = document.createElement("div");
    arrow.className = "scroll-arrow";
    arrow.textContent = "→";
    outer.appendChild(arrow);
    let scrollTimer;
    updateOverflow();
    wrapper.addEventListener("scroll", () => {
      outer.classList.add("scrolling");
      clearTimeout(scrollTimer);
      checkScrollEnd();
    });
    const ro = new ResizeObserver(updateOverflow);
    ro.observe(wrapper);
    function checkScrollEnd() {
      clearTimeout(scrollTimer);
      scrollTimer = setTimeout(() => {
        outer.classList.remove("scrolling");
        updateOverflow();
      }, 350);
    }
    function updateOverflow() {
      if (wrapper.scrollWidth > wrapper.clientWidth) {
        outer.classList.add("overflow");
        const atEnd = wrapper.scrollLeft + wrapper.clientWidth >= wrapper.scrollWidth - 1;
        outer.classList.toggle("scrolled-end", atEnd);
      } else {
        outer.classList.remove("overflow", "scrolled-end", "scrolling");
      }
    }
  });
}
function render(el, html) {
  html = html.replace(/<img\s/gi, '<img loading="lazy" ');
  html = html.replace(/<iframe\s/gi, '<iframe loading="lazy" ');
  html = html.replace(/<video\s/gi, '<video preload="none" ');
  el.innerHTML = html;
  wrapTables(el);
  highlight(el);
}

// streaming.js
import { marked } from "marked";
var el = null;
var content = "";
var thinking = "";
var streamId = 0;
var renderPending = false;
var onFinalize = null;
function setOnFinalize(cb) {
  onFinalize = cb;
}
function getEl() {
  if (!el) {
    const row = document.createElement("div");
    row.className = "message-row assistant";
    const avatar = document.createElement("div");
    avatar.className = "msg-avatar";
    avatar.innerHTML = BOT_AVATAR;
    row.appendChild(avatar);
    el = document.createElement("div");
    el.className = "msg-content chat-markdown";
    row.appendChild(el);
    els.messages.appendChild(row);
  }
  return el;
}
function scheduleRender(id) {
  if (id !== streamId)
    return;
  if (renderPending)
    return;
  renderPending = true;
  requestAnimationFrame(() => {
    renderPending = false;
    if (streamId !== id)
      return;
    const target = getEl();
    let html = "";
    if (thinking) {
      html += '<div class="thinking-block"><details open><summary>+ thoughts</summary><pre>' + escapeHtml(thinking) + "</pre></details></div>";
    }
    if (content) {
      html += marked.parse(content);
    }
    render(target, html);
    target.scrollIntoView({ behavior: "smooth", block: "end" });
  });
}
function handleThinking(text) {
  showTyping();
  thinking += text;
  scheduleRender(streamId);
}
function handleChunk(text) {
  hideTyping();
  content += text;
  scheduleRender(streamId);
}
function finalize() {
  if (!el) {
    if (onFinalize)
      onFinalize();
    return;
  }
  const target = el;
  let html = "";
  if (thinking) {
    html += '<div class="thinking-block"><details><summary>+ thoughts</summary><pre>' + escapeHtml(thinking) + "</pre></details></div>";
  }
  const trimmed = content.trim();
  if (trimmed) {
    target.className = "msg-content chat-markdown";
    html += marked.parse(trimmed);
    render(target, html);
  } else if (thinking) {
    target.className = "msg-content";
    target.textContent = thinking;
  }
  el = null;
  content = "";
  thinking = "";
  streamId++;
  if (onFinalize)
    onFinalize();
}
function finalizeIfActive() {
  if (thinking || content)
    finalize();
}
function handleDone(text) {
  hideTyping();
  if (text)
    content = text;
  finalize();
}
function reset() {
  if (el) {
    const parent = el.parentElement;
    if (parent)
      parent.remove();
    el = null;
  }
  content = "";
  thinking = "";
  streamId++;
}
function showTyping() {
  if (els.typingEl) {
    els.typingEl.classList.remove("hidden");
    els.typingEl.classList.add("active");
  }
}
function hideTyping() {
  if (els.typingEl) {
    els.typingEl.classList.add("hidden");
    els.typingEl.classList.remove("active");
  }
}

// messages.js
var abortController = null;
function extractMessageText(raw) {
  if (!raw)
    return "";
  if (raw.startsWith("{")) {
    try {
      const p = JSON.parse(raw);
      if (p.message)
        return p.message;
    } catch (e) {}
  }
  return raw;
}
function buildMessageRow(role, text) {
  const row = document.createElement("div");
  row.className = "message-row " + role;
  if (role === "error") {
    const content2 = document.createElement("div");
    content2.className = "msg-content";
    content2.textContent = text;
    row.appendChild(content2);
  } else if (role === "user") {
    const avatar = document.createElement("div");
    avatar.className = "msg-avatar";
    avatar.innerHTML = USER_AVATAR;
    row.appendChild(avatar);
    const content2 = document.createElement("div");
    content2.className = "msg-content";
    content2.textContent = text;
    row.appendChild(content2);
  } else {
    const avatar = document.createElement("div");
    avatar.className = "msg-avatar";
    avatar.innerHTML = BOT_AVATAR;
    row.appendChild(avatar);
    const content2 = document.createElement("div");
    content2.className = "msg-content chat-markdown";
    render(content2, marked2.parse(text || ""));
    row.appendChild(content2);
  }
  return row;
}
function addMessage(role, text, scroll = true) {
  finalize();
  const row = buildMessageRow(role, text);
  els.messages.appendChild(row);
  if (els.hint && els.hint.style.display !== "none")
    els.hint.style.display = "none";
  if (scroll) {
    row.scrollIntoView({ behavior: "smooth", block: "end" });
  }
}
function scrollToBottom() {
  const lastMsg = els.messages.lastElementChild;
  if (lastMsg) {
    lastMsg.scrollIntoView({ block: "end" });
  }
}
function fetchHistory() {
  if (abortController)
    abortController.abort();
  abortController = new AbortController;
  const phone = localStorage.getItem("zerowrapper_phone");
  if (!phone)
    return;
  const url2 = "/api/history?user_id=" + encodeURIComponent(phone);
  fetch(url2, { signal: abortController.signal }).then((r) => r.json()).then((data) => {
    if (data.session_id) {
      localStorage.setItem("zerowrapper_session", data.session_id);
    }
    if (!data.messages || data.messages.length === 0)
      return;
    const frag = document.createDocumentFragment();
    data.messages.forEach((msg) => {
      if (msg.role === "user" || msg.role === "assistant") {
        const row = buildMessageRow(msg.role, msg.role === "assistant" ? msg.content : extractMessageText(msg.content));
        if (row)
          frag.appendChild(row);
      }
    });
    els.messages.appendChild(frag);
    scrollToBottom();
  }).catch(() => {});
}
function abortHistoryFetch() {
  if (abortController) {
    abortController.abort();
    abortController = null;
  }
}

// tool-calls.js
var pending = [];
function handleToolCall(id, name, args) {
  const pill = document.createElement("div");
  pill.className = "tool-call-pill";
  pill.dataset.id = id || "";
  const argsStr = args ? JSON.stringify(args, null, 2) : null;
  pill.innerHTML = '<div class="tc-pill-inner">' + "+ acting" + '<span class="tc-pill-status running">◌</span>' + "</div>" + (argsStr ? '<div class="tc-pill-detail hidden"><pre>' + escapeHtml(argsStr) + "</pre></div>" : "");
  pill.addEventListener("click", () => {
    const detail = pill.querySelector(".tc-pill-detail");
    if (detail)
      detail.classList.toggle("hidden");
  });
  els.messages.appendChild(pill);
  pill.scrollIntoView({ behavior: "smooth", block: "end" });
  pending.push({ id: id || "", el: pill, name });
}
function handleToolResult(id, name, output) {
  const idx = pending.findIndex((p) => p.id === id);
  if (idx === -1)
    return;
  const item = pending[idx];
  pending.splice(idx, 1);
  const status = item.el.querySelector(".tc-pill-status");
  if (status) {
    status.className = "tc-pill-status done";
    status.innerHTML = "✓";
  }
  if (output) {
    let detail = item.el.querySelector(".tc-pill-detail");
    const existed = !!detail;
    if (!detail) {
      detail = document.createElement("div");
      detail.className = "tc-pill-detail hidden";
      item.el.appendChild(detail);
    }
    if (output.length <= 80) {
      detail.textContent = output;
    } else {
      detail.innerHTML = "<details><summary>" + output.slice(0, 80) + "...</summary><pre>" + escapeHtml(output) + "</pre></details>";
    }
    if (!existed) {
      item.el.addEventListener("click", () => {
        detail.classList.toggle("hidden");
      });
    }
  }
  item.el.scrollIntoView({ behavior: "smooth", block: "end" });
}
function clearPending() {
  pending = [];
}

// connection.js
var onStateChange = null;
function setHint(state3) {
  if (!els.hint)
    return;
  const fallback = els.hintText ? els.hintText.dataset.fallback || els.hintText.textContent : "";
  switch (state3) {
    case State.CONNECTING:
      els.hint.style.display = "flex";
      if (els.hintText)
        els.hintText.textContent = "Connecting to backend…";
      if (els.chatRefreshBtn)
        els.chatRefreshBtn.classList.add("hidden");
      break;
    case State.CONNECTED:
      els.hint.style.display = "none";
      break;
    default:
      els.hint.style.display = "flex";
      if (els.hintText)
        els.hintText.textContent = fallback;
      if (els.chatRefreshBtn)
        els.chatRefreshBtn.classList.remove("hidden");
  }
}
on("stateChange", setHint);
function handleMessage(e) {
  try {
    const data = JSON.parse(e.data);
    switch (data.type) {
      case "session_created":
        if (data.session_id && data.session_id !== state2.sessionId) {
          state2.sessionId = data.session_id;
          saveToStorage();
        }
        break;
      case "thinking":
        handleThinking(data.content);
        break;
      case "chunk":
        handleChunk(data.content);
        break;
      case "chunk_reset":
        break;
      case "tool_call":
        finalizeIfActive();
        handleToolCall(data.tool_call_id, data.name, data.args);
        break;
      case "tool_result":
        handleToolResult(data.tool_call_id, data.name, data.output);
        break;
      case "done":
        handleDone(data.content);
        if (onStateChange)
          onStateChange("done");
        break;
      case "error":
        addMessage("error", data.message || data.error || "Unknown error");
        if (onStateChange)
          onStateChange("error");
        break;
    }
  } catch (e2) {
    addMessage("assistant", e.data);
  }
}
function connect2() {
  if (getState() !== State.DISCONNECTED)
    disconnect();
  abortHistoryFetch();
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const url2 = protocol + "//" + location.host + "/ws";
  connect(url2, {
    onOpen: () => {
      fetchHistory();
    },
    onMessage: handleMessage,
    onClose: (e) => {
      reset();
      clearPending();
      if (onStateChange)
        onStateChange("disconnect");
    }
  });
}
function disconnect2() {
  disconnect();
  reset();
  clearPending();
  abortHistoryFetch();
}
function send2(text, role, attachments) {
  if (getState() !== State.CONNECTED) {
    addMessage("error", "Not connected to backend.");
    return;
  }
  const payload = {
    session_id: state2.sessionId || "",
    user_id: state2.phone,
    content: text
  };
  if (role)
    payload.role = role;
  if (attachments && attachments.length > 0)
    payload.attachments = attachments;
  send(JSON.stringify(payload));
}
function sendCancel2() {
  sendCancel(state2.sessionId);
}

// lightbox.js
var lightbox = null;
var loadingEl = null;
var images = [];
var currentIndex = 0;
var touchStartX = 0;
function ensureLightbox() {
  if (!lightbox) {
    lightbox = document.createElement("div");
    lightbox.id = "lightbox";
    lightbox.innerHTML = '<button class="lb-close">&times;</button><button class="lb-prev">&#10094;</button><button class="lb-next">&#10095;</button>';
    loadingEl = document.createElement("div");
    loadingEl.className = "lb-loading";
    loadingEl.textContent = "Loading...";
    lightbox.appendChild(loadingEl);
    document.body.appendChild(lightbox);
    lightbox.addEventListener("click", (e) => {
      if (e.target === lightbox || e.target.classList.contains("lb-close"))
        close();
    });
    lightbox.addEventListener("click", (e) => {
      const btn = e.target.closest(".lb-prev, .lb-next");
      if (!btn)
        return;
      btn.classList.contains("lb-prev") ? navigate(-1) : navigate(1);
    });
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape")
        close();
      if (e.key === "ArrowLeft")
        navigate(-1);
      if (e.key === "ArrowRight")
        navigate(1);
    });
    lightbox.addEventListener("touchstart", (e) => {
      touchStartX = e.changedTouches[0].screenX;
    }, { passive: true });
    lightbox.addEventListener("touchend", (e) => {
      const delta = touchStartX - e.changedTouches[0].screenX;
      if (delta > 50)
        navigate(1);
      else if (delta < -50)
        navigate(-1);
    }, { passive: true });
  }
  return lightbox;
}
function close() {
  if (lightbox) {
    lightbox.classList.remove("active");
    document.body.style.overflow = "";
    const img = lightbox.querySelector("img");
    if (img) {
      img.onload = null;
      img.onerror = null;
    }
  }
}
function navigate(dir) {
  const next = currentIndex + dir;
  if (next < 0 || next >= images.length)
    return;
  currentIndex = next;
  loadImage(images[currentIndex]);
  updateNavButtons();
}
function loadImage(src) {
  const lb = ensureLightbox();
  let img = lb.querySelector("img");
  if (!img) {
    img = document.createElement("img");
    lb.appendChild(img);
  }
  img.style.display = "none";
  img.onload = null;
  img.onerror = null;
  if (loadingEl)
    loadingEl.style.display = "block";
  img.onload = () => {
    img.style.display = "block";
    if (loadingEl)
      loadingEl.style.display = "none";
  };
  img.onerror = () => {
    if (loadingEl)
      loadingEl.textContent = "Failed to load image";
    setTimeout(() => {
      if (loadingEl)
        loadingEl.textContent = "Loading...";
    }, 2000);
  };
  lb.classList.add("active");
  document.body.style.overflow = "hidden";
  img.src = src;
}
function updateNavButtons() {
  if (!lightbox)
    return;
  const prev = lightbox.querySelector(".lb-prev");
  const next = lightbox.querySelector(".lb-next");
  if (prev)
    prev.style.display = currentIndex <= 0 ? "none" : "flex";
  if (next)
    next.style.display = currentIndex >= images.length - 1 ? "none" : "flex";
}
function init2(messagesEl) {
  messagesEl.addEventListener("click", (e) => {
    if (e.target.tagName === "IMG" && e.target.closest(".chat-markdown")) {
      const container = e.target.closest(".chat-markdown");
      const imgs = Array.from(container.querySelectorAll("img"));
      images = imgs.map((img) => img.src);
      currentIndex = imgs.indexOf(e.target);
      if (currentIndex === -1)
        currentIndex = 0;
      ensureLightbox();
      updateNavButtons();
      loadImage(images[currentIndex]);
    }
  });
}

// file-upload.js
var uploadState = {
  pending: []
};
async function uploadFile(file) {
  try {
    const formData = new FormData;
    formData.append("file", file);
    const res = await fetch("/api/upload", {
      method: "POST",
      headers: { "X-Filename": file.name },
      body: file
    });
    const data = await res.json();
    if (data.error)
      throw new Error(data.error);
    return data;
  } catch (err) {
    throw err;
  }
}
function createAttachmentChip(fileInfo) {
  const chip = document.createElement("div");
  chip.className = "attachment-chip";
  chip.style.cssText = "display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 6px; border: 1px solid var(--color-border-weak); background: var(--color-bg-weak); font-size: 0.75rem; margin-bottom: 4px;";
  chip.innerHTML = `
    <span style="color: var(--color-text-strong);">${fileInfo.filename}</span>
    <span style="color: var(--color-text-weak); font-size: 0.625rem;">${formatSize(fileInfo.size)}</span>
    <button class="remove-attachment" style="background: none; border: none; cursor: pointer; color: var(--color-status-error); font-size: 0.875rem; padding: 0; line-height: 1;">&times;</button>
  `;
  chip.querySelector(".remove-attachment").addEventListener("click", () => {
    chip.remove();
    uploadState.pending = uploadState.pending.filter((a) => a.path !== fileInfo.path);
  });
  return chip;
}
function formatSize(bytes) {
  if (bytes < 1024)
    return bytes + " B";
  if (bytes < 1024 * 1024)
    return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

// owner-chat.js
import { marked as marked3 } from "marked";
var state3 = {
  sessionId: "",
  phone: "",
  role: ""
};
var streamingContent = "";
var streamingEl = null;
var onFinalize2 = null;
var typingTimer = null;
var pendingToolCalls = new Map;
var thinkingContent = "";
function getEl2(id) {
  return document.getElementById(id);
}
function getContainer() {
  return getEl2("owner-messages");
}
function getTyping() {
  return getEl2("owner-typing");
}
function getHint() {
  return getEl2("owner-hint");
}
function showOwnerDisconnected() {
  const hint = getHint();
  if (!hint)
    return;
  const p = getEl2("owner-hint-text");
  if (p)
    p.textContent = "Backend not connected. Check your connection and refresh.";
  const btn = getEl2("owner-refresh-btn");
  if (btn)
    btn.classList.remove("hidden");
  hint.style.display = "flex";
}
function extractAttachments(raw) {
  if (!raw || !raw.startsWith("{"))
    return null;
  try {
    const p = JSON.parse(raw);
    if (p.attachments && Array.isArray(p.attachments) && p.attachments.length > 0) {
      return p.attachments;
    }
  } catch (e) {}
  return null;
}
function extractMessageText2(raw) {
  if (!raw)
    return "";
  if (raw.startsWith("{")) {
    try {
      const p = JSON.parse(raw);
      if (p.message)
        return p.message;
    } catch (e) {}
  }
  return raw;
}
function renderTextWithAttachments(text) {
  const div = document.createElement("div");
  const attachments = extractAttachments(text);
  const msgText = extractMessageText2(text);
  if (attachments) {
    const fileList = document.createElement("div");
    fileList.className = "msg-attachments";
    attachments.forEach((a) => {
      const chip = document.createElement("span");
      chip.className = "attachment-chip";
      chip.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/><polyline points="14 2 14 8 20 8"/></svg> ' + escapeHtml(a.filename || a.name || "file");
      fileList.appendChild(chip);
    });
    div.appendChild(fileList);
  }
  if (msgText) {
    const p = document.createElement("p");
    p.style.margin = attachments ? "6px 0 0" : "0";
    p.textContent = msgText;
    div.appendChild(p);
  }
  return div;
}
function buildMessageRow2(role, text) {
  const row = document.createElement("div");
  row.className = "message-row " + role;
  if (role === "error") {
    const content2 = document.createElement("div");
    content2.className = "msg-content";
    content2.textContent = text;
    row.appendChild(content2);
  } else if (role === "user") {
    const avatar = document.createElement("div");
    avatar.className = "msg-avatar";
    avatar.innerHTML = USER_AVATAR;
    row.appendChild(avatar);
    const content2 = document.createElement("div");
    content2.className = "msg-content";
    content2.appendChild(renderTextWithAttachments(text));
    row.appendChild(content2);
  } else {
    const avatar = document.createElement("div");
    avatar.className = "msg-avatar";
    avatar.innerHTML = BOT_AVATAR;
    row.appendChild(avatar);
    const content2 = document.createElement("div");
    content2.className = "msg-content chat-markdown";
    render(content2, marked3.parse(text || ""));
    row.appendChild(content2);
  }
  return row;
}
function addMessage2(role, text) {
  const container = getContainer();
  if (!container)
    return;
  const hint = getHint();
  if (hint)
    hint.style.display = "none";
  const row = buildMessageRow2(role, text);
  container.appendChild(row);
  row.scrollIntoView({ block: "end" });
}
function showTyping2() {
  const el2 = getTyping();
  if (el2)
    el2.classList.remove("hidden");
}
function hideTyping2() {
  const el2 = getTyping();
  if (el2)
    el2.classList.add("hidden");
}
function resetStreaming() {
  streamingContent = "";
  streamingEl = null;
  thinkingContent = "";
  if (typingTimer) {
    clearTimeout(typingTimer);
    typingTimer = null;
  }
  hideTyping2();
}
function finalizeStreaming() {
  if (streamingEl) {
    const text = streamingContent;
    const thinkText = thinkingContent;
    streamingContent = "";
    streamingEl = null;
    thinkingContent = "";
    const container = getContainer();
    const existing = container ? container.querySelector(".message-row.streaming") : null;
    if (existing)
      existing.remove();
    if (thinkText || text.trim()) {
      const row = document.createElement("div");
      row.className = "message-row assistant";
      const avatar = document.createElement("div");
      avatar.className = "msg-avatar";
      avatar.innerHTML = BOT_AVATAR;
      row.appendChild(avatar);
      const content2 = document.createElement("div");
      content2.className = "msg-content chat-markdown";
      let html = "";
      if (thinkText) {
        html += '<div class="thinking-block"><details><summary>+ thoughts</summary><pre>' + escapeHtml(thinkText) + "</pre></details></div>";
      }
      if (text.trim()) {
        html += marked3.parse(text.trim());
      }
      render(content2, html);
      row.appendChild(content2);
      if (container) {
        container.appendChild(row);
        row.scrollIntoView({ block: "end" });
      }
    }
  }
}
function handleThinking2(content2) {
  thinkingContent += content2 || "";
  showTyping2();
  if (typingTimer)
    clearTimeout(typingTimer);
  typingTimer = setTimeout(() => {
    hideTyping2();
  }, 30000);
}
function handleChunk2(content2) {
  hideTyping2();
  if (typingTimer) {
    clearTimeout(typingTimer);
    typingTimer = null;
  }
  if (!streamingEl) {
    streamingContent = "";
    thinkingContent = "";
    streamingEl = document.createElement("div");
    streamingEl.className = "message-row assistant streaming";
    const avatar = document.createElement("div");
    avatar.className = "msg-avatar";
    avatar.innerHTML = BOT_AVATAR;
    streamingEl.appendChild(avatar);
    const contentDiv2 = document.createElement("div");
    contentDiv2.className = "msg-content chat-markdown";
    streamingEl.appendChild(contentDiv2);
    const container = getContainer();
    if (container) {
      container.appendChild(streamingEl);
      streamingEl.scrollIntoView({ block: "end" });
    }
  }
  streamingContent += content2;
  const contentDiv = streamingEl.querySelector(".msg-content");
  if (contentDiv) {
    let html = "";
    if (thinkingContent) {
      html += '<div class="thinking-block"><details open><summary>+ thoughts</summary><pre>' + escapeHtml(thinkingContent) + "</pre></details></div>";
    }
    if (streamingContent) {
      html += marked3.parse(streamingContent);
    }
    render(contentDiv, html);
  }
}
function handleDone2(content2) {
  if (content2 && content2.trim()) {
    streamingContent = content2;
  }
  finalizeStreaming();
  if (onFinalize2) {
    onFinalize2();
    onFinalize2 = null;
  }
  document.dispatchEvent(new CustomEvent("owner-stream-done"));
}
function handleToolCall2(id, name, args) {
  finalizeStreaming();
  const container = getContainer();
  if (!container)
    return;
  const hint = getHint();
  if (hint)
    hint.style.display = "none";
  const row = document.createElement("div");
  row.className = "message-row tool-call";
  row.dataset.toolCallId = id;
  const avatar = document.createElement("div");
  avatar.className = "msg-avatar";
  avatar.innerHTML = BOT_AVATAR;
  row.appendChild(avatar);
  const content2 = document.createElement("div");
  content2.className = "msg-content";
  const argsStr = typeof args === "string" ? args : JSON.stringify(args, null, 2);
  content2.innerHTML = '<details class="tool-call-details">' + '<summary><span class="tc-pill-status running">◌</span> + acting</summary>' + '<p style="margin:4px 0 2px;font-weight:600;font-size:0.8rem;">' + escapeHtml(name) + "</p>" + "<pre>" + escapeHtml(argsStr) + "</pre>" + "</details>";
  row.appendChild(content2);
  container.appendChild(row);
  row.scrollIntoView({ block: "end" });
  pendingToolCalls.set(id, row);
}
function handleToolResult2(id, name, output) {
  const row = pendingToolCalls.get(id);
  if (!row)
    return;
  const details = row.querySelector(".tool-call-details");
  if (details) {
    const summary = details.querySelector("summary");
    if (summary) {
      summary.innerHTML = '<span class="tc-pill-status done">✓</span> + acting';
    }
    if (output) {
      const existingPre = details.querySelector("pre");
      if (existingPre)
        existingPre.remove();
      const pre = document.createElement("pre");
      if (output.length > 80) {
        pre.innerHTML = "<details><summary>" + escapeHtml(output.slice(0, 80)) + "...</summary><pre>" + escapeHtml(output) + "</pre></details>";
      } else {
        pre.textContent = output;
      }
      details.appendChild(pre);
    }
  }
  pendingToolCalls.delete(id);
}
function ownerFetchHistory() {
  if (!state3.phone)
    return;
  const url2 = "/api/history?user_id=" + encodeURIComponent(state3.phone) + "&role=" + encodeURIComponent(state3.role);
  fetch(url2).then((r) => r.json()).then((data) => {
    if (data.session_id) {
      state3.sessionId = data.session_id;
    }
    if (!data.messages || data.messages.length === 0)
      return;
    const container = getContainer();
    if (!container)
      return;
    const hint = getHint();
    if (hint)
      hint.style.display = "none";
    const frag = document.createDocumentFragment();
    data.messages.forEach((msg) => {
      if (msg.role === "user" || msg.role === "assistant") {
        const text = msg.role === "assistant" ? msg.content : msg.content;
        frag.appendChild(buildMessageRow2(msg.role, text));
      }
    });
    container.appendChild(frag);
    const last = container.lastElementChild;
    if (last)
      last.scrollIntoView({ block: "end" });
  }).catch(() => {});
}
function connectOwnerWs() {
  if (getState() !== State.DISCONNECTED)
    disconnect();
  resetStreaming();
  pendingToolCalls.clear();
  const refreshBtn = getEl2("owner-refresh-btn");
  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => location.reload());
  }
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const url2 = protocol + "//" + location.host + "/ws";
  connect(url2, {
    onOpen: () => {
      const hint = getHint();
      if (hint)
        hint.style.display = "none";
      const p = getEl2("owner-hint-text");
      if (p)
        p.textContent = "Connected. Send a message to start.";
      const btn = getEl2("owner-refresh-btn");
      if (btn)
        btn.classList.add("hidden");
      ownerFetchHistory();
    },
    onMessage: (e) => {
      try {
        const data = JSON.parse(e.data);
        switch (data.type) {
          case "session_created":
            if (data.session_id && data.session_id !== state3.sessionId)
              state3.sessionId = data.session_id;
            break;
          case "thinking":
            handleThinking2(data.content);
            break;
          case "chunk":
            handleChunk2(data.content);
            break;
          case "tool_call":
            handleToolCall2(data.tool_call_id, data.name, data.args);
            break;
          case "tool_result":
            handleToolResult2(data.tool_call_id, data.name, data.output);
            break;
          case "done":
            handleDone2(data.content);
            break;
          case "error":
            addMessage2("error", data.message || data.error || "Unknown error");
            break;
        }
      } catch (e2) {
        addMessage2("assistant", e2.data);
      }
    },
    onClose: () => {
      resetStreaming();
      pendingToolCalls.clear();
      showOwnerDisconnected();
    }
  });
}
function disconnectOwnerWs() {
  disconnect();
  resetStreaming();
  pendingToolCalls.clear();
}
function sendOwnerMessage(text, attachments) {
  if (getState() !== State.CONNECTED) {
    addMessage2("error", "Not connected to backend.");
    return;
  }
  const payload = {
    session_id: state3.sessionId || "",
    user_id: state3.phone,
    content: text,
    role: state3.role
  };
  if (attachments && attachments.length > 0) {
    payload.attachments = attachments.map((a) => ({ filename: a.filename, mime_type: a.mime_type, path: a.path }));
  }
  send(JSON.stringify(payload));
}
function sendOwnerCancel() {
  sendCancel(state3.sessionId);
}
function setOnFinalize2(fn) {
  onFinalize2 = fn;
}

// owner.js
var STORAGE_PHONE2 = "zerowrapper_owner_phone";
var STORAGE_NAME2 = "zerowrapper_owner_name";
var STORAGE_ROLE = "zerowrapper_owner_role";
var STORAGE_SESSION2 = "zerowrapper_owner_session";
var state4 = {
  phone: "",
  displayName: "",
  role: ""
};
function loadFromStorage2() {
  state4.phone = localStorage.getItem(STORAGE_PHONE2) || "";
  state4.displayName = localStorage.getItem(STORAGE_NAME2) || "";
  state4.role = localStorage.getItem(STORAGE_ROLE) || "";
  state3.sessionId = localStorage.getItem(STORAGE_SESSION2) || "";
}
function saveToStorage2() {
  if (state4.phone)
    localStorage.setItem(STORAGE_PHONE2, state4.phone);
  if (state4.displayName)
    localStorage.setItem(STORAGE_NAME2, state4.displayName);
  if (state4.role)
    localStorage.setItem(STORAGE_ROLE, state4.role);
  if (state3.sessionId)
    localStorage.setItem(STORAGE_SESSION2, state3.sessionId);
}
function clearFromStorage2() {
  localStorage.removeItem(STORAGE_PHONE2);
  localStorage.removeItem(STORAGE_NAME2);
  localStorage.removeItem(STORAGE_ROLE);
  localStorage.removeItem(STORAGE_SESSION2);
}
function init3() {
  document.getElementById("owner-btn")?.addEventListener("click", showLogin);
  document.getElementById("owner-btn-mobile")?.addEventListener("click", showLogin);
  document.getElementById("owner-login-close")?.addEventListener("click", hideLogin);
  document.getElementById("owner-login-btn")?.addEventListener("click", handleLogin);
  document.getElementById("owner-back-btn")?.addEventListener("click", backToPublic);
  document.getElementById("owner-password-input")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter")
      document.getElementById("owner-login-btn")?.click();
  });
  loadFromStorage2();
  if (state4.phone && state4.role) {
    showOwnerChat();
  }
  document.addEventListener("owner-stream-done", showOwnerSendIcon);
}
function setupOwnerForm() {
  const form = document.getElementById("owner-chat-form");
  const input = document.getElementById("owner-chat-input");
  const cancelBtn = document.getElementById("owner-cancel-btn");
  const uploadBtn = document.getElementById("owner-upload-btn");
  const fileInput = document.getElementById("owner-file-input");
  if (!form)
    return;
  form._ownerHandler = (e) => {
    e.preventDefault();
    if (getState() !== State.CONNECTED) {
      addMessage2("error", "Not connected.");
      return;
    }
    finalizeStreaming();
    const text = input.value.trim();
    const attachments = [...uploadState.pending];
    if (!text && attachments.length === 0) {
      addMessage2("error", "Cannot send an empty message.");
      return;
    }
    showOwnerStopIcon();
    const combined = text && attachments.length > 0 ? JSON.stringify({ message: text, attachments: attachments.map((a) => ({ filename: a.filename, mime_type: a.mime_type, path: a.path })) }) : attachments.length > 0 ? JSON.stringify({ attachments: attachments.map((a) => ({ filename: a.filename, mime_type: a.mime_type, path: a.path })) }) : text;
    addMessage2("user", combined);
    sendOwnerMessage(text, attachments);
    showTyping2();
    setOnFinalize2(showOwnerSendIcon);
    input.value = "";
    input.style.height = "auto";
    uploadState.pending = [];
    const attachContainer = document.getElementById("owner-attachments");
    if (attachContainer)
      attachContainer.innerHTML = "";
  };
  form.addEventListener("submit", form._ownerHandler);
  input.addEventListener("input", autoGrow);
  if (cancelBtn) {
    cancelBtn.addEventListener("click", () => {
      sendOwnerCancel();
      showOwnerSendIcon();
    });
  }
  if (uploadBtn && fileInput) {
    uploadBtn.addEventListener("click", () => fileInput.click());
    fileInput.addEventListener("change", async () => {
      const files = Array.from(fileInput.files || []);
      for (const file of files) {
        try {
          const info = await uploadFile(file);
          uploadState.pending.push(info);
          const chip = createAttachmentChip(info);
          document.getElementById("owner-attachments")?.appendChild(chip);
        } catch (err) {
          addMessage2("error", "Upload failed: " + err.message);
        }
      }
      fileInput.value = "";
    });
  }
}
function teardownOwnerForm() {
  const form = document.getElementById("owner-chat-form");
  if (form && form._ownerHandler) {
    form.removeEventListener("submit", form._ownerHandler);
    delete form._ownerHandler;
  }
}
function autoGrow() {
  const input = document.getElementById("owner-chat-input");
  if (input) {
    input.style.height = "auto";
    input.style.height = Math.min(input.scrollHeight, 300) + "px";
  }
}
function showOwnerSendIcon() {
  const sendBtn = document.getElementById("owner-send-btn");
  const cancelBtn = document.getElementById("owner-cancel-btn");
  if (sendBtn)
    sendBtn.style.display = "flex";
  if (cancelBtn)
    cancelBtn.style.display = "none";
}
function showOwnerStopIcon() {
  const sendBtn = document.getElementById("owner-send-btn");
  const cancelBtn = document.getElementById("owner-cancel-btn");
  if (sendBtn)
    sendBtn.style.display = "none";
  if (cancelBtn)
    cancelBtn.style.display = "flex";
}
function showLogin() {
  document.getElementById("owner-login-overlay")?.classList.remove("hidden");
}
function hideLogin() {
  document.getElementById("owner-login-overlay")?.classList.add("hidden");
  document.getElementById("owner-phone-input").value = "";
  document.getElementById("owner-password-input").value = "";
  const err = document.getElementById("owner-login-error");
  if (err)
    err.classList.add("hidden");
}
async function handleLogin() {
  const phone = document.getElementById("owner-phone-input").value.trim();
  const password = document.getElementById("owner-password-input").value.trim();
  const errEl = document.getElementById("owner-login-error");
  if (!phone || !password) {
    if (errEl) {
      errEl.textContent = "Please enter phone and password";
      errEl.classList.remove("hidden");
    }
    return;
  }
  try {
    const res = await fetch("/api/admin/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ phone, password })
    });
    const data = await res.json();
    if (!data.ok) {
      if (errEl) {
        errEl.textContent = data.error || "Login failed";
        errEl.classList.remove("hidden");
      }
      return;
    }
    state4.phone = data.phone;
    state4.displayName = data.name;
    state4.role = data.role;
    saveToStorage2();
    hideLogin();
    showOwnerChat();
  } catch (err) {
    const errEl2 = document.getElementById("owner-login-error");
    if (errEl2) {
      errEl2.textContent = "Network error";
      errEl2.classList.remove("hidden");
    }
  }
}
function showOwnerChat() {
  disconnect2();
  const chatEl = document.getElementById("owner-chat");
  if (!chatEl) {
    console.error("[owner] owner-chat element not found");
    return;
  }
  document.getElementById("page-root")?.classList.add("hidden");
  chatEl.classList.remove("hidden");
  const badge = document.getElementById("owner-role-badge");
  if (badge)
    badge.textContent = state4.role;
  state3.phone = state4.phone;
  state3.role = state4.role;
  connectOwnerWs();
  setupOwnerForm();
}
function backToPublic() {
  disconnectOwnerWs();
  teardownOwnerForm();
  clearFromStorage2();
  document.getElementById("owner-chat")?.classList.add("hidden");
  document.getElementById("page-root")?.classList.remove("hidden");
  const container = document.getElementById("chat-messages");
  if (container)
    container.innerHTML = '<p id="fallback-hint" class="text-xs text-center" style="color: var(--color-text-weak);">Ask AI a question...</p>';
  const attachContainer = document.getElementById("owner-attachments");
  if (attachContainer)
    attachContainer.innerHTML = "";
  uploadState.pending = [];
}

// main.js
function logPendingResources() {
  const entries = performance.getEntriesByType("resource");
  const pending2 = entries.filter((e) => !e.responseEnd || e.responseEnd <= 0);
  if (pending2.length) {
    console.warn("[resources] pending:", pending2.map((r) => r.name));
  } else {
    console.log("[resources] all", entries.length, "resources loaded");
  }
  const fonts = document.fonts ? document.fonts.status : "n/a";
  console.log("[resources] readyState:", document.readyState, "fonts:", fonts);
}
function init4() {
  init();
  logPendingResources();
  setTimeout(logPendingResources, 3000);
  init3();
  if (els.chatRefreshBtn) {
    els.chatRefreshBtn.addEventListener("click", () => location.reload());
  }
  if (!els.form)
    return;
  init2(els.messages);
  loadFromStorage();
  if (state2.phone) {
    showBadge();
    connect2();
  } else {
    showForm();
  }
  els.registerBtn.addEventListener("click", handleRegister);
  els.phoneInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter")
      els.registerBtn.click();
  });
  els.nameInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter")
      els.registerBtn.click();
  });
  els.logoutBtn.addEventListener("click", handleLogout);
  if (els.exampleChips) {
    els.exampleChips.addEventListener("click", (e) => {
      const btn = e.target.closest(".example-chip");
      if (!btn)
        return;
      if (!state2.phone) {
        scrollToAuth();
        return;
      }
      els.input.value = btn.getAttribute("data-query") || "";
      autoGrow2();
      els.form.dispatchEvent(new Event("submit", { cancelable: true }));
    });
  }
  els.form.addEventListener("submit", handleSubmit);
  els.input.addEventListener("input", autoGrow2);
  els.cancelBtn.addEventListener("click", handleCancel);
  setOnFinalize(showSendIcon);
}
function showSendIcon() {
  if (els.sendBtn)
    els.sendBtn.style.display = "flex";
  if (els.cancelBtn)
    els.cancelBtn.style.display = "none";
}
function showStopIcon() {
  if (els.sendBtn)
    els.sendBtn.style.display = "none";
  if (els.cancelBtn)
    els.cancelBtn.style.display = "flex";
}
function handleCancel() {
  sendCancel2();
  showSendIcon();
}
async function handleRegister() {
  hideAuthError();
  const code = els.countryCode.value;
  const raw = els.phoneInput.value.trim();
  const name = els.nameInput.value.trim();
  if (!raw) {
    showAuthError("Please enter your phone number.");
    return;
  }
  const normalizedPhone = normalizePhone(code, raw);
  try {
    const loginRes = await fetch("/api/login?phone=" + encodeURIComponent(normalizedPhone));
    const loginData = await loginRes.json();
    if (loginData.exists) {
      state2.phone = loginData.phone;
      state2.displayName = loginData.name || loginData.phone;
    } else {
      const regRes = await fetch("/api/register", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ phone: normalizedPhone, name: name || undefined })
      });
      const regData = await regRes.json();
      if (!regData.ok) {
        showAuthError(regData.error || "Registration failed.");
        return;
      }
      state2.phone = regData.phone;
      state2.displayName = regData.name || regData.phone;
    }
    saveToStorage();
    showBadge();
    connect2();
  } catch (err) {
    showAuthError("Network error. Please try again.");
  }
}
function handleLogout() {
  clearFromStorage();
  state2.phone = "";
  state2.displayName = "";
  state2.sessionId = "";
  disconnect2();
  els.messages.innerHTML = "";
  if (els.hint)
    els.hint.style.display = "block";
  if (els.hintText)
    els.hintText.textContent = els.hintText.dataset.fallback || els.hintText.textContent;
  if (els.chatRefreshBtn)
    els.chatRefreshBtn.classList.add("hidden");
  showForm();
}
function handleSubmit(e) {
  e.preventDefault();
  if (!state2.phone) {
    scrollToAuth();
    return;
  }
  finalizeIfActive();
  const text = els.input.value.trim();
  if (!text)
    return;
  if (getState() !== State.CONNECTED) {
    addMessage("error", "Not connected to backend.");
    return;
  }
  showStopIcon();
  addMessage("user", text);
  send2(text);
  showTyping();
  els.input.value = "";
  els.input.style.height = "auto";
}
function autoGrow2() {
  els.input.style.height = "auto";
  els.input.style.height = Math.min(els.input.scrollHeight, 300) + "px";
}

// app-entry.js
init4();
