# Docs

Documentation framed as a type-specimen book: each page opens with its title
shown at specimen scale, the sidebar is the book's index, and the reading
column stays calm. Search, an outline of each page, and previous and next
links are built from your markdown; there is no menu to maintain.

```bash
statico dev
statico check
statico build https://docs.your-domain.example   # also writes search.json
```

## Make it yours

| File | What to change |
|---|---|
| `statico.json` | project name, description, repository link |
| `collections/sections.json` | sidebar sections and their hairline colour |
| `content/docs/*.md` | pages: title, description, section, order |
| `includes/note.html` | the note / tip / warning include |

The demo pages document the template itself; read them, then replace them.

Fonts: Bricolage Grotesque and Hanken Grotesk, both SIL Open Font License (see
`assets/fonts/`).
