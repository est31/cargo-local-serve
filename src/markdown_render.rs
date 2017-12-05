use pulldown_cmark::{html, Parser, Event, Tag};
use ammonia::Builder;
use syntect_format::SyntectFormatter;
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
	static ref AMMONIA_BUILDER :Builder<'static> = construct_ammonia_builder();
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

					let fmt = SyntectFormatter::new().token(token);
					let formatted = fmt.highlight_snippet(&text_buf);
					return Some(Event::Html(Cow::Owned(formatted)));
				},
				_ => panic!("Unexpected element inside codeblock mode {:?}", next),
			}
		}
		Some(next)
	}
}

fn construct_ammonia_builder() -> Builder<'static> {
	use std::iter;
	let mut r = Builder::default();
	// TODO: filter out everything that can have scr attributes.
	// TODO: maybe replace all img's with their alt text?
	r.rm_tags(iter::once("img"));
	// TODO: do filtering of inline CSS
	// (or even better: output classes instead of inline css)
	r.add_tag_attributes("span", iter::once("style"));
	r
}

/// Renders a given markdown string to sanitized HTML
/// with formatted code blocks.
pub fn render_markdown(markdown :&str) -> String {
	let p = Parser::new(&markdown);
	let ev_it = EventIter::new(p);
	let mut unsafe_html = String::new();
	html::push_html(&mut unsafe_html, ev_it);
	let safe_html = AMMONIA_BUILDER.clean(&unsafe_html).to_string();
	safe_html
}
