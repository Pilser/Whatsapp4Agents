import * as network from './network.js'
import { renderBoard } from './whiteboard-render.js'

const SWIPE_THRESHOLD = 40

let sessionId = ''
let mode = 'closed'
let panel = null
let body = null
let pins = null
let down = null
let up = null
let dragStartY = null
let dragging = false

function getEl(id) { return document.getElementById(id) }

function getInputTop() {
  const input = activeInput()
  if (!input) return window.innerHeight * 0.6
  return input.getBoundingClientRect().top
}

function applyMode() {
  if (!panel) return
  panel.style.transform = ''
  panel.classList.remove('wb-closed', 'wb-peek', 'wb-full')
  if (mode === 'peek') {
    const offset = -(window.innerHeight - getInputTop())
    panel.style.setProperty('--wb-peek-offset', offset + 'px')
    panel.classList.add('wb-peek')
  } else if (mode === 'full') {
    panel.classList.add('wb-full')
  } else {
    panel.classList.add('wb-closed')
  }
}

function setMode(next) {
  if (next === mode) return
  mode = next
  applyMode()
}

function cycleDown() {
  if (mode === 'closed') setMode('peek')
  else if (mode === 'peek') setMode('full')
}

function cycleUp() {
  if (mode === 'full') setMode('peek')
  else if (mode === 'peek') setMode('closed')
}

function activeInput() {
  const input = getEl('chat-input') || getEl('owner-chat-input')
  if (!input) return null
  const r = input.getBoundingClientRect()
  if (r.width > 0 && r.height > 0) return input
  const other = input.id === 'chat-input' ? getEl('owner-chat-input') : getEl('chat-input')
  if (other) {
    const r2 = other.getBoundingClientRect()
    if (r2.width > 0 && r2.height > 0) return other
  }
  return input
}

function positionArrows() {
  if (!down || !up) return
  const input = activeInput()
  if (!input) return
  const r = input.getBoundingClientRect()
  const cx = r.left + r.width / 2 + 55
  const top = r.top - 48
  const pos = (btn, x) => {
    btn.style.left = (cx + x) + 'px'
    btn.style.top = top + 'px'
    btn.style.right = 'auto'
    btn.style.bottom = 'auto'
  }
  pos(down, -40)
  pos(up, 6)
}

function wireButton(btn, onTap) {
  if (!btn) return
  btn.addEventListener('click', (e) => {
    if (!dragging) onTap()
  })
  btn.addEventListener('pointerdown', (e) => {
    dragStartY = e.clientY
    dragging = true
    btn.setPointerCapture(e.pointerId)
  })
  btn.addEventListener('pointermove', (e) => {
    if (!dragging || dragStartY === null || !panel) return
    const dy = e.clientY - dragStartY
    if (mode === 'closed') panel.style.transform = `translateY(${Math.min(dy, 120)}px)`
    else if (mode === 'full') panel.style.transform = `translateY(${Math.max(dy, -120)}px)`
  })
  const endDrag = (e) => {
    if (!dragging) return
    const dy = e.clientY - dragStartY
    dragStartY = null
    dragging = false
    if (dy > SWIPE_THRESHOLD) cycleDown()
    else if (dy < -SWIPE_THRESHOLD) cycleUp()
    else applyMode()
  }
  btn.addEventListener('pointerup', endDrag)
  btn.addEventListener('pointercancel', endDrag)
}

export function init() {
  panel = getEl('whiteboard-panel')
  body = getEl('whiteboard-body')
  pins = getEl('whiteboard-pins')
  down = getEl('whiteboard-down')
  up = getEl('whiteboard-up')

  if (!panel) return

  wireButton(down, cycleDown)
  wireButton(up, cycleUp)

  positionArrows()
  window.addEventListener('resize', positionArrows)
  window.addEventListener('scroll', positionArrows, { passive: true })
  document.addEventListener('visibilitychange', () => { if (!document.hidden) positionArrows() })
  new MutationObserver(positionArrows)
    .observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] })

  applyMode()
}

export function sync() {
  if (!sessionId) return
  network.send(JSON.stringify({ type: 'whiteboard_sync', session_id: sessionId }))
}

export function setSession(sid) {
  if (!sid) return
  if (sid !== sessionId) {
    sessionId = sid
    sync()
  }
}

export function setBoard(data) {
  if (!data) return
  if (data.session_id) sessionId = data.session_id
  renderBoard(body, pins, data)
}
