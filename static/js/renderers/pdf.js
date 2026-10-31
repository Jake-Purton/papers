import { api } from '../common.js'

// v1: the browser's own pdf viewer. It brings search, zoom and printing for free,
// but it's opaque to us, so there's no position tracking or theming yet.
export async function open(stage, book) {
    const frame = document.createElement('iframe')
    frame.className = 'pdf-frame'
    frame.title = book.title
    frame.src = api.fileUrl(book.id)
    stage.append(frame)

    return {
        navigation: false,
        appearance: false,
        toc: [],
    }
}
