import hljs from 'highlight.js'
import { marked } from 'marked'
import katex from 'katex'
import mermaid from 'mermaid'

function escapeHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

marked.use({
  extensions: [
    {
      name: 'mathBlock',
      level: 'block',
      start(src) { return src.indexOf('$$') },
      tokenizer(src) {
        const match = /^\$\$([\s\S]+?)\$\$\n?/.exec(src)
        if (!match) return undefined
        return { type: 'mathBlock', raw: match[0], text: match[1] }
      },
      renderer(token) {
        try {
          return `<div class="math-block">${katex.renderToString(token.text, { displayMode: true, throwOnError: false })}</div>`
        } catch {
          return `<div class="math-block">${escapeHtml(token.text)}</div>`
        }
      },
    },
    {
      name: 'mathInline',
      level: 'inline',
      start(src) { return src.indexOf('$') },
      tokenizer(src) {
        const match = /^\$([^\s$][^$\n]*[^\s$])\$/.exec(src)
        if (!match) return undefined
        return { type: 'mathInline', raw: match[0], text: match[1] }
      },
      renderer(token) {
        try {
          return katex.renderToString(token.text, { displayMode: false, throwOnError: false })
        } catch {
          return escapeHtml(token.text)
        }
      },
    },
    {
      name: 'mermaid',
      level: 'block',
      start(src) { return src.indexOf('```mermaid') },
      tokenizer(src) {
        const match = /^```mermaid\s*\n([\s\S]*?)```/.exec(src)
        if (!match) return undefined
        return { type: 'mermaid', raw: match[0], text: match[1] }
      },
      renderer(token) {
        return `<div class="mermaid">${escapeHtml(token.text)}</div>`
      },
    },
  ],
})

mermaid.initialize({ startOnLoad: false, theme: 'base' })

function renderMermaid(el) {
  const nodes = el.querySelectorAll('div.mermaid')
  if (!nodes.length) return
  try {
    mermaid.run({ nodes: Array.from(nodes) }).catch(() => {})
  } catch {
    /* ignore */
  }
}

export function highlight(el) {
  el.querySelectorAll('pre code').forEach((block) => {
    hljs.highlightElement(block)
  })
}

export function wrapTables(el) {
  el.querySelectorAll('table').forEach((t) => {
    if (t.parentElement && t.parentElement.classList.contains('table-wrap')) return
    const outer = document.createElement('div')
    outer.className = 'table-outer-wrap'
    const wrapper = document.createElement('div')
    wrapper.className = 'table-wrap'
    t.parentNode.insertBefore(outer, t)
    outer.appendChild(wrapper)
    wrapper.appendChild(t)

    const fade = document.createElement('div')
    fade.className = 'scroll-fade'
    outer.appendChild(fade)
    const arrow = document.createElement('div')
    arrow.className = 'scroll-arrow'
    arrow.textContent = '\u2192'
    outer.appendChild(arrow)

    let scrollTimer
    updateOverflow()
    wrapper.addEventListener('scroll', () => {
      outer.classList.add('scrolling')
      clearTimeout(scrollTimer)
      checkScrollEnd()
    })
    const ro = new ResizeObserver(updateOverflow)
    ro.observe(wrapper)

    function checkScrollEnd() {
      clearTimeout(scrollTimer)
      scrollTimer = setTimeout(() => {
        outer.classList.remove('scrolling')
        updateOverflow()
      }, 350)
    }
    function updateOverflow() {
      if (wrapper.scrollWidth > wrapper.clientWidth) {
        outer.classList.add('overflow')
        const atEnd = wrapper.scrollLeft + wrapper.clientWidth >= wrapper.scrollWidth - 1
        outer.classList.toggle('scrolled-end', atEnd)
      } else {
        outer.classList.remove('overflow', 'scrolled-end', 'scrolling')
      }
    }
  })
}

export function render(el, html) {
  html = html.replace(/<img\s/gi, '<img loading="lazy" ')
  html = html.replace(/<iframe\s/gi, '<iframe loading="lazy" ')
  html = html.replace(/<video\s/gi, '<video preload="none" ')
  el.innerHTML = html
  wrapTables(el)
  highlight(el)
  renderMermaid(el)
}
