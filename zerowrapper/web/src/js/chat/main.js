import { els } from './dom.js'
import * as dom from './dom.js'
import * as auth from './auth.js'
import * as connection from './connection.js'
import * as streaming from './streaming.js'
import { addMessage } from './messages.js'
import * as lightbox from './lightbox.js'

export function init() {
  dom.init()
  lightbox.init(els.messages)

  auth.loadFromStorage()

  if (auth.state.phone) {
    auth.showBadge()
    connection.connect()
  } else {
    auth.showForm()
  }

  els.registerBtn.addEventListener('click', handleRegister)
  els.phoneInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') els.registerBtn.click() })
  els.nameInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') els.registerBtn.click() })

  els.logoutBtn.addEventListener('click', handleLogout)

  if (els.exampleChips) {
    els.exampleChips.addEventListener('click', (e) => {
      const btn = e.target.closest('.example-chip')
      if (!btn) return
      if (!auth.state.phone) { auth.scrollToAuth(); return }
      els.input.value = btn.getAttribute('data-query') || ''
      autoGrow()
      els.form.dispatchEvent(new Event('submit', { cancelable: true }))
    })
  }

  els.form.addEventListener('submit', handleSubmit)
  els.input.addEventListener('input', autoGrow)
  els.cancelBtn.addEventListener('click', handleCancel)
  streaming.setOnFinalize(showSendIcon)
}

function showSendIcon() {
  if (els.sendBtn) els.sendBtn.style.display = 'flex'
  if (els.cancelBtn) els.cancelBtn.style.display = 'none'
}

function showStopIcon() {
  if (els.sendBtn) els.sendBtn.style.display = 'none'
  if (els.cancelBtn) els.cancelBtn.style.display = 'flex'
}

function handleCancel() {
  connection.sendCancel()
  showSendIcon()
}

async function handleRegister() {
  auth.hideAuthError()
  const code = els.countryCode.value
  const raw = els.phoneInput.value.trim()
  const name = els.nameInput.value.trim()
  if (!raw) { auth.showAuthError('Please enter your phone number.'); return }
  const normalizedPhone = auth.normalizePhone(code, raw)

  try {
    const loginRes = await fetch('/api/login?phone=' + encodeURIComponent(normalizedPhone))
    const loginData = await loginRes.json()

    if (loginData.exists) {
      auth.state.phone = loginData.phone
      auth.state.displayName = loginData.name || loginData.phone
    } else {
      const regRes = await fetch('/api/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ phone: normalizedPhone, name: name || undefined }),
      })
      const regData = await regRes.json()
      if (!regData.ok) {
        auth.showAuthError(regData.error || 'Registration failed.')
        return
      }
      auth.state.phone = regData.phone
      auth.state.displayName = regData.name || regData.phone
    }
    auth.saveToStorage()
    auth.showBadge()
    connection.connect()
  } catch (err) {
    auth.showAuthError('Network error. Please try again.')
  }
}

function handleLogout() {
  auth.clearFromStorage()
  auth.state.phone = ''
  auth.state.displayName = ''
  auth.state.sessionId = ''
  connection.disconnect()
  els.messages.innerHTML = ''
  if (els.hint) els.hint.style.display = 'block'
  auth.showForm()
}

function handleSubmit(e) {
  e.preventDefault()
  if (!auth.state.phone) { auth.scrollToAuth(); return }
  streaming.finalizeIfActive()
  const text = els.input.value.trim()
  if (!text) return
  if (!connection.state.connected) { addMessage('error', 'Not connected to backend.'); return }
  showStopIcon()
  addMessage('user', text)
  connection.send(text)
  streaming.showTyping()
  els.input.value = ''
  els.input.style.height = 'auto'
}

function autoGrow() {
  els.input.style.height = 'auto'
  els.input.style.height = Math.min(els.input.scrollHeight, 300) + 'px'
}
