import '../../vendor/foliate-js/view.js'
import { api } from '../common.js'

// injected into every chapter; !important on colours so the theme wins over the book's own css
const css = ({ page, ink, accent, scheme, fontSize }) => `
    @namespace epub "http://www.idpf.org/2007/ops";
    html {
        color-scheme: ${scheme};
        font-family: "Iowan Old Style", "Charter", "Palatino Linotype", Palatino, Georgia, serif;
        font-size: ${fontSize}% !important;
    }
    html, body {
        color: ${ink} !important;
        background: ${page} !important;
    }
    a:link, a:visited { color: ${accent} !important; }
    p, li, blockquote, dd {
        line-height: 1.55;
        hyphens: auto;
        -webkit-hyphens: auto;
        widows: 2;
        orphans: 2;
    }
    pre { white-space: pre-wrap !important; }
    aside[epub|type~="footnote"], aside[epub|type~="endnote"], aside[epub|type~="rearnote"] { display: none; }
`

export async function open(stage, book, { location, appearance, onRelocate, onKeydown }) {
    const view = document.createElement('foliate-view')
    stage.append(view)

    await view.open(api.fileUrl(book.id))

    const setAppearance = ({ flow, ...colors }) => {
        view.renderer.setAttribute('flow', flow)
        view.renderer.setStyles?.(css(colors))
    }
    setAppearance(appearance)

    view.addEventListener('relocate', ({ detail }) => onRelocate({
        location: detail.cfi,
        fraction: detail.fraction,
        label: detail.tocItem?.label,
        href: detail.tocItem?.href,
    }))

    // keys pressed while focus is inside a chapter iframe
    view.addEventListener('load', ({ detail: { doc } }) => doc.addEventListener('keydown', onKeydown))

    await view.init({ lastLocation: location })

    return {
        navigation: true,
        appearance: true,
        toc: view.book.toc ?? [],
        left: () => view.goLeft(),
        right: () => view.goRight(),
        next: () => view.next(),
        prev: () => view.prev(),
        goTo: href => view.goTo(href),
        goToFraction: fraction => view.goToFraction(fraction),
        setAppearance,
    }
}
