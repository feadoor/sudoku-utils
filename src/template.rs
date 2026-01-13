#[derive(Debug)]
pub enum TemplateDigit {
    Empty,
    Given(u8),
    Wildcard(Vec<u8>),
}

#[derive(Debug)]
pub struct Template([TemplateDigit; 81]);

impl Template {
    pub fn from_str(s: &str) -> Self {
        let mut digits = Vec::with_capacity(81);
        let mut chars = s.chars();
        while let Some(digit) = Self::next_digit(&mut chars) { digits.push(digit); }
        Self(digits.try_into().unwrap())
    }

    pub fn digits(&self) -> impl Iterator<Item = &TemplateDigit> {
        self.0.iter()
    }

    fn next_digit<I: Iterator<Item = char>>(chars: &mut I) -> Option<TemplateDigit> {
        chars.next().map(|c| match c {
            d @ '1' ..= '9' => TemplateDigit::Given(d.to_digit(10).unwrap() as u8),
            '[' | '(' | '{' | '<' => {
                let mut digits = Vec::new();
                while let Some(d) = chars.next().filter(|&d| '1' <= d && d <= '9') { digits.push(d.to_digit(10).unwrap() as u8); }
                TemplateDigit::Wildcard(digits)
            },
            _ => TemplateDigit::Empty,
        })
    }
}
