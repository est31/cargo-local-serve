use pulldown_cmark::{html, Parser, Event, Tag};
#[allow(unused_imports)]
use ammonia::clean;
use syntect::html::highlighted_snippet_for_string;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use std::borrow::Cow;

struct EventIter<'a> {
	p :Parser<'a>,
}

impl<'a> EventIter<'a> {
	pub fn new(p :Parser<'a>) -> Self {
		EventIter {
			p,
		}
	}
}

lazy_static! {
	static ref THEME_SET :ThemeSet = ThemeSet::load_defaults();
}

impl<'a> Iterator for EventIter<'a> {
	type Item = Event<'a>;

	fn next(&mut self) -> Option<Self::Item> {
		let next = if let Some(v) = self.p.next() {
			v
		} else {
			return None;
		};
		if let &Event::Start(Tag::CodeBlock(_)) = &next {
			// Codeblock time!
			let mut text_buf = String::new();
			let mut next = self.p.next();
			loop {
				if let Some(Event::Text(ref s)) = next {
					text_buf += s;
				} else {
					break;
				}
				next = self.p.next();
			}
			match &next {
				&Some(Event::End(Tag::CodeBlock(ref token))) => {

					thread_local!(static SYN_SET :SyntaxSet = SyntaxSet::load_defaults_newlines());

					let theme = &THEME_SET.themes["base16-ocean.dark"];

					return SYN_SET.with(|s| {
						if let Some(syntax) = s.find_syntax_by_token(token) {
							// TODO find a way to avoid inline css in the syntect formatter
							let formatted = highlighted_snippet_for_string(&text_buf,
								&syntax, theme);
							return Some(Event::Html(Cow::Owned(formatted)));
						} else {
							let code_block = format!("<pre><code>{}</code></pre>",
								text_buf);
							return Some(Event::Html(Cow::Owned(code_block)));
						}
					});
				},
				_ => panic!("Unexpected element inside codeblock mode {:?}", next),
			}
		}
		Some(next)
	}
}

/// Renders a given markdown string to sanitized HTML
/// with formatted code blocks.
pub fn render_markdown(markdown :&str) -> String {
	let p = Parser::new(&markdown);
	let ev_it = EventIter::new(p);
	let mut unsafe_html = String::new();
	html::push_html(&mut unsafe_html, ev_it);
	//let safe_html = clean(&unsafe_html);
	unsafe_html
}
