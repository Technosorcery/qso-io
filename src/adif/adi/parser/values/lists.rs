use std::{borrow::Cow, collections::HashSet};

use super::{DataType, ParseEncoding, ValueError, ValueFailure, identifiers, source_range};
use crate::adif::{
    Award, AwardSponsor, Credit, QslMedium, SecondarySubdivision, SecondarySubdivisionAlt,
    text::check_text,
};

pub(super) fn parse<'a>(
    kind: DataType,
    text: Cow<'a, str>,
    raw: &'a [u8],
    encoding: ParseEncoding,
) -> Result<(Vec<Cow<'a, str>>, Cow<'a, str>), ValueFailure> {
    let (separator, minimum, cause) = match kind {
        DataType::AwardList => (',', 1, ValueError::InvalidAwardList),
        DataType::CreditList => (',', 1, ValueError::InvalidCreditList),
        DataType::SponsoredAwardList => (',', 1, ValueError::InvalidSponsoredAwardList),
        DataType::GridSquareList => (',', 1, ValueError::InvalidGridSquareList),
        DataType::POTARefList => (',', 1, ValueError::InvalidPOTARefList),
        DataType::SecondarySubdivisionList => (':', 2, ValueError::InvalidSecondarySubdivisionList),
        DataType::SecondaryAdministrativeSubdivisionListAlt => (
            ';',
            1,
            ValueError::InvalidSecondaryAdministrativeSubdivisionListAlt,
        ),
        _ => (',', 1, ValueError::InvalidString),
    };
    let fail = |range, enumeration| ValueFailure {
        cause,
        span: source_range(&text, range, encoding),
        enumeration,
    };
    check_text(raw, encoding, false).map_err(|span| ValueFailure {
        cause,
        span,
        enumeration: None,
    })?;

    let mut offset = 0;
    let mut count = 0;
    let mut names = HashSet::new();
    for member in text.split(separator) {
        let end = offset + member.len();
        if member.is_empty() {
            return Err(fail(offset..end, None));
        }
        match kind {
            DataType::AwardList => {
                member
                    .parse::<Award>()
                    .map_err(|_| fail(offset..end, Some("Award")))?;
            }
            DataType::CreditList => {
                let (credit, media) = match member.split_once(':') {
                    Some((credit, media)) => (credit, Some(media)),
                    None => (member, None),
                };
                credit
                    .parse::<Credit>()
                    .map_err(|_| fail(offset..offset + credit.len(), Some("Credit")))?;
                if let Some(media) = media {
                    let mut token_offset = offset + credit.len() + 1;
                    for medium in media.split('&') {
                        medium.parse::<QslMedium>().map_err(|_| {
                            fail(
                                token_offset..token_offset + medium.len(),
                                Some("QSL_Medium"),
                            )
                        })?;
                        token_offset += medium.len() + 1;
                    }
                }
            }
            DataType::SponsoredAwardList => {
                let parts: Vec<_> = member.splitn(3, '_').collect();
                if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
                    return Err(fail(offset..end, None));
                }
                if parts[0].parse::<AwardSponsor>().is_err() {
                    return Err(fail(
                        offset..offset + parts[0].len() + 1,
                        Some("Award_Sponsor"),
                    ));
                }
            }
            DataType::SecondarySubdivisionList => {
                if SecondarySubdivision::candidates(member).next().is_none() {
                    return Err(fail(
                        offset..end,
                        Some("Secondary_Administrative_Subdivision"),
                    ));
                }
            }
            DataType::SecondaryAdministrativeSubdivisionListAlt => {
                let Some((name, code)) = member.split_once(':') else {
                    return Err(fail(end..end, None));
                };
                if name.is_empty() || code.is_empty() || !names.insert(name.to_ascii_uppercase()) {
                    return Err(fail(offset..end, None));
                }
                if SecondarySubdivisionAlt::parse_member(member).is_none() {
                    return Err(fail(
                        offset..end,
                        Some("Secondary_Administrative_Subdivision_Alt"),
                    ));
                }
            }
            DataType::GridSquareList | DataType::POTARefList => {
                let member_kind = if kind == DataType::GridSquareList {
                    DataType::GridSquare
                } else {
                    DataType::POTARef
                };
                let raw_span = source_range(&text, offset..end, encoding);
                identifiers::check(member_kind, member, &raw[raw_span.clone()], encoding).map_err(
                    |e| ValueFailure {
                        cause: e.cause,
                        span: raw_span.start + e.span.start..raw_span.start + e.span.end,
                        enumeration: e.enumeration,
                    },
                )?;
            }
            _ => {}
        }
        count += 1;
        offset = end + separator.len_utf8();
    }
    if count < minimum {
        return Err(fail(text.len()..text.len(), None));
    }

    let members = match &text {
        Cow::Borrowed(source) => source.split(separator).map(Cow::Borrowed).collect(),
        Cow::Owned(source) => source
            .split(separator)
            .map(|part| Cow::Owned(part.to_owned()))
            .collect(),
    };
    Ok((members, text))
}
