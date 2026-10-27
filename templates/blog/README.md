# Blog

A blog framed as a railway timetable: the home page is a table of every post
(its departure date, title, lines and reading time), tags are coloured lines
with their own pages, and each post is a calm reading column.

```bash
statico dev
statico check
statico build https://your-domain.example   # the URL also turns on the Atom feed
```

## Make it yours

| File | What to change |
|---|---|
| `statico.json` | blog name, description, author |
| `content/posts/*.md` | posts: title, description, date, tags |
| `collections/lines.json` | lines (tags): name, title, one-letter code, colour |
| `pages/about.html` | about you |

The demo posts explain the template itself; read them, then replace them.

Fonts: Archivo and Literata, both SIL Open Font License (see `assets/fonts/`).
