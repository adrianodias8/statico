"""Split docs/REFERENCE.md into one documentation page per topic.

The reference is the single source for both `statico docs` and this site.
Every `## topic` must have an entry in PAGES, so a new topic can't be added
to the reference without a place in the book.

usage: python3 site/reference.py docs/REFERENCE.md <site>/content/docs
"""

import json
import re
import sys
from pathlib import Path

# topic: (title, section, order, description)
PAGES = {
    "project": ("Project structure", "start", 4, "The folders of a Statico site and what each one is for."),
    "layouts": ("Layouts", "pages", 5, "Wrap pages in shared shells, pass them attributes, and nest layouts."),
    "includes": ("Includes", "pages", 6, "Reusable pieces of HTML, with attributes, in pages, layouts and markdown."),
    "placeholders": ("Placeholders", "pages", 7, "Insert front matter, site settings, collections and URLs with {braces}."),
    "filters": ("Filters", "pages", 8, "Format dates, truncate, slugify and more, right inside a placeholder."),
    "loops": ("Loops", "pages", 9, "Repeat markup over collections and markdown content with the ForEach tag."),
    "conditions": ("Conditions", "pages", 10, "Show markup only when a condition holds, with a safe expression language."),
    "content": ("Markdown content", "content", 11, "Write pages in markdown with front matter, drafts and tables of contents."),
    "collections": ("Collections", "content", 12, "Data from JSON, YAML, TOML or CSV files, ready to loop over."),
    "pagination": ("Pagination", "content", 13, "Split long lists across pages, with links between them."),
    "taxonomies": ("Taxonomies", "content", 14, "One page per tag, category or any other front matter field."),
    "feeds": ("Feeds", "content", 15, "Atom feeds for your content folders, written automatically."),
    "seo": ("SEO", "content", 16, "Meta tags, Open Graph, Twitter cards and JSON-LD from one placeholder."),
    "schemas": ("Schemas", "content", 17, "Validate front matter per content folder, so mistakes fail the build."),
    "build": ("Building", "ship", 18, "What statico build writes: pages, assets, sitemap, feeds, search and more."),
    "config": ("Configuration", "ship", 19, "Every statico.json setting, all of them optional."),
    "templates": ("Templates", "ship", 20, "The starter templates in the binary, and using any repository or folder."),
    "cli": ("Command line", "ship", 21, "Every statico command, its flags and its exit codes."),
    "agents": ("AI agents", "ref", 23, "How Statico is built to be driven by AI coding agents through its CLI."),
    "errors": ("Error codes", "ref", 24, "Every stable error and warning code, with what it means."),
}


def main(reference: str, out_dir: str) -> None:
    # Split on `## topic` headings, ignoring any inside fenced code examples.
    sections, name, body, fenced = [], None, [], False
    for line in Path(reference).read_text(encoding="utf-8").splitlines():
        if line.startswith("```"):
            fenced = not fenced
        m = None if fenced else re.match(r"^## (\w[\w-]*)\s*$", line)
        if m:
            if name:
                sections.append((name, "\n".join(body)))
            name, body = m.group(1), []
        elif name:
            body.append(line)
    if name:
        sections.append((name, "\n".join(body)))

    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    seen = set()
    for name, body in sections:
        if name not in PAGES:
            sys.exit(f"reference.py: topic `{name}` has no entry in PAGES")
        title, section, order, about = PAGES[name]
        page = (
            # json.dumps quotes and escapes, and a JSON string is valid YAML.
            f"---\ntitle: {json.dumps(title)}\ndescription: {json.dumps(about)}\n"
            f"section: {section}\norder: {order}\n---\n\n"
            + body.strip()
            + "\n"
        )
        (out / f"{name}.md").write_text(page, encoding="utf-8")
        seen.add(name)
    missing = set(PAGES) - seen
    if missing:
        sys.exit(f"reference.py: no `## {sorted(missing)[0]}` in {reference}")
    print(f"reference.py: wrote {len(seen)} pages")


if __name__ == "__main__":
    main(*sys.argv[1:3])
