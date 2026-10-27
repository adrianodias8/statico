# Portfolio

A freelancer's site, set as a Brussels shopfront: the name lettered in gilt on
the fascia, the trade on the window, and the door as the way to book a call.
Below it: services, career record, a project, and writing.

```bash
statico dev       # preview at http://localhost:3000
statico check     # links, alt text, front matter
statico build https://your-domain.example
```

## Make it yours

| File | What to change |
|---|---|
| `statico.json` | name, description, email, booking link, GitHub |
| `collections/services.json` | what you offer |
| `collections/experience.json` | where you've worked |
| `collections/projects.json` | something you built |
| `collections/menu.json` | navigation |
| `content/writing/*.md` | posts (title, description, date) |
| `pages/about.html` | your bio |
| `assets/images/` | your photos |

The palette and type are tokens at the top of `assets/css/main.css`.

This copy ships with Adriano Dias's real content as the example. Replace it all.

Fonts: Bodoni Moda and Schibsted Grotesk, both SIL Open Font License (see `assets/fonts/`).
