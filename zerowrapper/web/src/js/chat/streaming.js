import { marked } from 'marked'
import { els } from './dom.js'
import { BOT_AVATAR, THINK_ICON, escapeHtml } from './constants.js'
import { render as renderMarkdown } from './markdown.js'

let el = null
let content = ''
let thinking = ''
let streamId = 0
let renderPending = false

function getEl() {
  if (!el) {
    const row = document.createElement('div')
    row.className = 'message-row assistant'
    const avatar = document.createElement('div')
    avatar.className = 'msg-avatar'
    avatar.innerHTML = BOT_AVATAR
    row.appendChild(avatar)
    el = document.createElement('div')
    el.className = 'msg-content chat-markdown'
    row.appendChild(el)
    els.messages.appendChild(row)
  }
  return el
}

function scheduleRender(id) {
  if (id !== streamId) return
  if (renderPending) return
  renderPending = true
  requestAnimationFrame(() => {
    renderPending = false
    if (streamId !== id) return
    const target = getEl()
    let html = ''
    if (thinking) {
      html += '<div class="thinking-block"><details open><summary>' + THINK_ICON + ' Thinking</summary><pre>' + escapeHtml(thinking) + '</pre></details></div>'
    }
    if (content) {
      html += marked.parse(content)
    }
    renderMarkdown(target, html)
    target.scrollIntoView({ behavior: 'smooth', block: 'end' })
  })
}

export function handleThinking(text) {
  hideTyping()
  thinking += text
  scheduleRender(streamId)
}

export function handleChunk(text) {
  hideTyping()
  content += text
  scheduleRender(streamId)
}

export function finalize() {
  if (!el) return
  const target = el
  let html = ''
  if (thinking) {
    html += '<div class="thinking-block"><details><summary>' + THINK_ICON + ' Thinking</summary><pre>' + escapeHtml(thinking) + '</pre></details></div>'
  }
  const trimmed = content.trim()
  if (trimmed) {
    target.className = 'msg-content chat-markdown'
    html += marked.parse(trimmed)
    renderMarkdown(target, html)
  } else if (thinking) {
    target.className = 'msg-content'
    target.textContent = thinking
  }
  el = null
  content = ''
  thinking = ''
  streamId++
}

export function finalizeIfActive() {
  if (thinking || content) finalize()
}

export function handleDone(text) {
  hideTyping()
  if (text) content = text
  finalize()
}

export function isActive() {
  return el !== null
}

export function reset() {
  if (el) {
    const parent = el.parentElement
    if (parent) parent.remove()
    el = null
  }
  content = ''
  thinking = ''
  streamId++
}

export function showTyping() {
  if (els.typingEl) els.typingEl.classList.add('active')
}

export function hideTyping() {
  if (els.typingEl) els.typingEl.classList.remove('active')
}
