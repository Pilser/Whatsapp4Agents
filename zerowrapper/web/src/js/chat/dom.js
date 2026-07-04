export const els = {}

export function init() {
  els.form = document.getElementById('chat-form')
  els.input = document.getElementById('chat-input')
  els.messages = document.getElementById('chat-messages')
  els.hint = document.getElementById('fallback-hint')
  els.typingEl = document.getElementById('typing-indicator')
  els.authSection = document.getElementById('auth-section')
  els.userBadge = document.getElementById('user-badge')
  els.badgeDisplay = document.getElementById('badge-display')
  els.registerBtn = document.getElementById('register-btn')
  els.phoneInput = document.getElementById('phone-input')
  els.nameInput = document.getElementById('name-input')
  els.countryCode = document.getElementById('country-code')
  els.authError = document.getElementById('auth-error')
  els.logoutBtn = document.getElementById('logout-btn')
  els.exampleChips = document.getElementById('example-chips')
}
