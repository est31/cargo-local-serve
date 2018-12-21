use std::fmt::{self, Write};
use syntect::html::IncludeBackground;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, Style, FontStyle, Color};
use escape::Escape;

pub fn highlight_string_snippet(s :&str, syntax :&SyntaxReference, theme :&Theme, syns :&SyntaxSet)
		-> String {
	let mut output = String::new();
	let mut highlighter = HighlightLines::new(syntax, theme);
	let c = theme.settings.background.unwrap_or(Color::WHITE);
	write!(output,
		"<pre style=\"background-color:#{:02x}{:02x}{:02x};\">\n",
		c.r,
		c.g,
		c.b).unwrap();
	let mut spcx = StyledPrintCx::new(IncludeBackground::IfDifferent(c));
	for line in s.lines() {
		let regions = highlighter.highlight(line, syns);
		spcx.styles_to_coloured_html(&mut output, &regions[..]);
		output.push('\n');
	}
	spcx.finish(&mut output);
	output.push_str("</pre>\n");
	output
}

struct SpanBegin<'a>(&'a Style, &'a IncludeBackground);

impl<'a> fmt::Display for SpanBegin<'a> {
	fn fmt(&self, fmt :&mut fmt::Formatter) -> fmt::Result {
		let style = self.0;
		let bg = self.1;

		try!(write!(fmt, "<span style=\""));
		let include_bg = match bg {
			&IncludeBackground::Yes => true,
			&IncludeBackground::No => false,
			&IncludeBackground::IfDifferent(c) => (style.background != c),
		};
		if include_bg {
			try!(write!(fmt, "background-color:"));
			try!(write_css_color(fmt, style.background));
			try!(write!(fmt, ";"));
		}
		if style.font_style.contains(FontStyle::UNDERLINE) {
			try!(write!(fmt, "text-decoration:underline;"));
		}
		if style.font_style.contains(FontStyle::BOLD) {
			try!(write!(fmt, "font-weight:bold;"));
		}
		if style.font_style.contains(FontStyle::ITALIC) {
			try!(write!(fmt, "font-style:italic;"));
		}
		try!(write!(fmt, "color:"));
		try!(write_css_color(fmt, style.foreground));
		try!(write!(fmt, ";\">"));

		Ok(())
	}
}

struct StyledPrintCx {
	background :IncludeBackground,
	prev_style :Option<Style>,
}

impl StyledPrintCx {
	fn new(bg :IncludeBackground) -> Self {
		StyledPrintCx {
			background : bg,
			prev_style : None,
		}
	}
	fn styles_to_coloured_html(&mut self, s :&mut String,
			v :&[(Style, &str)]) {
		for &(ref style, text) in v.iter() {
			let keep_style = if let Some(ref ps) = self.prev_style {
				style == ps ||
					(style.background == ps.background && text.trim().is_empty())
			} else {
				false
			};
			if keep_style {
				write!(s, "{}", Escape(text)).unwrap();
			} else {
				if self.prev_style.is_some() {
					write!(s, "</span>").unwrap();
				}
				self.prev_style = Some(*style);
				write!(s, "{}{}", SpanBegin(style, &self.background),
					Escape(text)).unwrap();
			}
		}
	}
	fn finish(&mut self, s :&mut String) {
		if self.prev_style.is_some() {
			write!(s, "</span>").unwrap();
		}
		self.prev_style = None;
	}
}

fn write_css_color(fmt :&mut fmt::Formatter, c :Color) -> fmt::Result {
	if c.a != 0xFF {
		try!(write!(fmt,"#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a));
	} else {
		try!(write!(fmt,"#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
	}
	Ok(())
}
