let lightbox = null

function ensureLightbox() {
  if (!lightbox) {
    lightbox = document.createElement('div')
    lightbox.id = 'lightbox'
    lightbox.innerHTML = '<button class="lb-close">&times;</button>'
    document.body.appendChild(lightbox)
    lightbox.addEventListener('click', (e) => {
      if (e.target === lightbox || e.target.classList.contains('lb-close')) close()
    })
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') close()
    })
  }
  return lightbox
}

function close() {
  if (lightbox) {
    lightbox.classList.remove('active')
    document.body.style.overflow = ''
  }
}

function open(src) {
  const lb = ensureLightbox()
  let img = lb.querySelector('img')
  if (!img) {
    img = document.createElement('img')
    lb.appendChild(img)
  }
  img.src = src
  lb.classList.add('active')
  document.body.style.overflow = 'hidden'
}

export function init(messagesEl) {
  messagesEl.addEventListener('click', (e) => {
    if (e.target.tagName === 'IMG' && e.target.closest('.chat-markdown')) {
      open(e.target.src)
    }
  })
}
