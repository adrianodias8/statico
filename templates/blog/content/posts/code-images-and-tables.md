---
title: Code, images and tables
description: A tour of everything a post can hold, so you can see how each one is set.
date: 2026-10-23
tags: [writing, reference]
---

[toc]

## Code

Fenced code blocks are highlighted when the site builds, with no script in the browser:

```rust
fn main() {
    let departures = ["07:12", "07:42", "08:12"];
    for time in departures {
        println!("Departs {time}");
    }
}
```

Inline code like `statico check` sits in the line.

## Images

Put images in `assets/images/` and always describe them; `statico check` warns about any image without alt text.

![Three coloured lines running left to right, crossing at a single station](/assets/images/lines.svg)

## Tables

| Line | Code | Use it for |
|---|---|---|
| Start here | S | posts a new reader should see first |
| Writing | W | notes on writing and editing |
| Reference | R | posts people will come back to |

## Quotes and notes

> A timetable is a promise made in advance. A blog is the same promise, kept one post at a time.

Footnotes work too.[^1] So do ~~strikethrough~~ and task lists:

- [x] Write the post
- [ ] Run `statico check`

[^1]: They appear at the bottom of the post, with a link back.
