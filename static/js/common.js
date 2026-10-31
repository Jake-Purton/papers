// ---------- api ----------

async function getJSON(url) {
    const res = await fetch(url)
    if (!res.ok) throw Object.assign(new Error(`${res.status} ${res.statusText}`), { status: res.status })
    return res.json()
}

export const api = {
    books: () => getJSON('/api/books'),
    book: id => getJSON(`/api/books/${encodeURIComponent(id)}`),
    fileUrl: id => `/api/books/${encodeURIComponent(id)}/file`,
    downloadUrl: id => `/api/books/${encodeURIComponent(id)}/file?download=1`,
}

// ---------- storage (best effort, private windows may throw) ----------

export const store = {
    get(key, fallback = null) {
        try {
            const v = localStorage.getItem(`papers:${key}`)
            return v === null ? fallback : JSON.parse(v)
        } catch { return fallback }
    },
    set(key, value) {
        try { localStorage.setItem(`papers:${key}`, JSON.stringify(value)) } catch {}
    },
}

// reading position per book, { location, fraction }
export const progress = {
    get: id => store.get(`pos:${id}`),
    set: (id, pos) => store.set(`pos:${id}`, pos),
}

// ---------- theme ----------

export function getTheme() {
    try { return localStorage.getItem('papers:theme') || 'auto' } catch { return 'auto' }
}

export function setTheme(theme) {
    try { localStorage.setItem('papers:theme', theme) } catch {}
    if (theme === 'auto') delete document.documentElement.dataset.theme
    else document.documentElement.dataset.theme = theme
}

export const prefersDark = () =>
    matchMedia('(prefers-color-scheme: dark)').matches

// ---------- formatting ----------

export function esc(s) {
    return String(s ?? '').replace(/[&<>"']/g, c =>
        ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c])
}

export function formatSize(bytes) {
    if (!bytes) return ''
    const units = ['B', 'KB', 'MB', 'GB']
    let i = 0
    while (bytes >= 1024 && i < units.length - 1) { bytes /= 1024; i++ }
    return `${bytes < 10 && i > 0 ? bytes.toFixed(1) : Math.round(bytes)} ${units[i]}`
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
const dateFmt = new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short', year: 'numeric' })

export function relativeTime(seconds) {
    if (!seconds) return ''
    const diff = seconds - Date.now() / 1000
    const abs = Math.abs(diff)
    if (abs < 60) return 'just now'
    if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute')
    if (abs < 86400) return rtf.format(Math.round(diff / 3600), 'hour')
    if (abs < 86400 * 7) return rtf.format(Math.round(diff / 86400), 'day')
    return dateFmt.format(new Date(seconds * 1000))
}

// ---------- covers ----------

function hue(s) {
    let h = 0
    for (const c of s) h = (h * 31 + c.codePointAt(0)) >>> 0
    return h % 360
}

function monogram(title) {
    const word = title.match(/[\p{L}\p{N}]/u)
    return word ? word[0].toUpperCase() : '•'
}

// generated covers sit underneath real ones, so a failed image falls back gracefully
export function coverHTML(book, { small = false } = {}) {
    const img = book.cover_url
        ? `<img src="${esc(book.cover_url)}" alt="" loading="lazy" decoding="async">`
        : ''
    return `<div class="cover${small ? ' small' : ''}" style="--h:${hue(book.title)}">
        <div class="gen" aria-hidden="true">
            <span class="gen-mono">${esc(monogram(book.title))}</span>
            <span class="gen-title">${esc(book.title)}</span>
            ${book.author ? `<span class="gen-rule"></span><span class="gen-author">${esc(book.author)}</span>` : ''}
            <span class="gen-format">${esc(book.format)}</span>
        </div>
        ${img}
    </div>`
}

// error events don't bubble, so listen in the capture phase
document.addEventListener('error', e => {
    if (e.target instanceof HTMLImageElement && e.target.parentElement?.classList.contains('cover')) e.target.remove()
}, true)

// ---------- icons (24px, stroke) ----------

const svg = d => `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`

export const icons = {
    search: svg('<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>'),
    x: svg('<path d="M18 6 6 18M6 6l12 12"/>'),
    list: svg('<path d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01"/>'),
    grid: svg('<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>'),
    sun: svg('<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>'),
    moon: svg('<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z"/>'),
    auto: svg('<circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 0 1 0 18Z" fill="currentColor"/>'),
    download: svg('<path d="M12 4v11M7 10l5 5 5-5M5 20h14"/>'),
    back: svg('<path d="M15 18l-6-6 6-6"/>'),
    left: svg('<path d="M15 18l-6-6 6-6"/>'),
    right: svg('<path d="M9 18l6-6-6-6"/>'),
    toc: svg('<path d="M4 6h16M4 12h10M4 18h13"/>'),
    type: svg('<path d="M4 7V5h11v2M9.5 5v14M7 19h5M14 13v-1.5h7V13M17.5 11.5V19M16 19h3"/>'),
    expand: svg('<path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/>'),
    book: svg('<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20V3H6.5A2.5 2.5 0 0 0 4 5.5v14Z"/><path d="M4 19.5A2.5 2.5 0 0 0 6.5 22H20v-5"/>'),
}
