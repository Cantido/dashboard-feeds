# dashboard-feeds

A small terminal application that fetches a list of RSS and Atom feeds.
It is designed for dashboard-style layouts, so it elegantly wraps lines on narrow panes.
It also embeds feed item links on terminals that support it.

## Usage

Create a config file at `~/.config/dashboard-feeds/config.kdl` with a list of URLs like this:

```kdl
feeds {
  url "https://blog.rust-lang.org/feed.xml"
  url "https://archlinux.org/feeds/news/"
}
```

Then run the program. You can add the `--limit` option (`-n` for short) to limit the number of returned posts.

```console
$ dashboard-feeds -n 5
- Rust Blog: Announcing Rust 1.87.0 and ten years of Rust!
- Rust Blog: Announcing Google Summer of Code 2025 selected projects
- Rust Blog: Announcing rustup 1.28.2
- Arch Linux: Recent news updates: Valkey to replace Redis in the [extra] Repository
- Rust Blog: crates.io security incident: improperly stored session cookies
```

## License

Copyright (C) 2025 Rosa Richter

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
