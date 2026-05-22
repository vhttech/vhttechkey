//! `spelling.go` — CVC validity matrices.

use crate::vi_engine::text::add_mark_to_toneless_char;

const FIRST_CONSONANT_SEQS: &[&str] = &[
    "b d đ g gh m n nh p ph r s t tr v z",
    "c h k kh qu th",
    "ch gi l ng ngh x",
    "đ l",
    "h",
];

const VOWEL_SEQS: &[&str] = &[
    "ê i ua uê uy y",
    "a iê oa uyê yê",
    "â ă e o oo ô ơ oe u ư uâ uô ươ",
    "oă",
    "uơ",
    "ai ao au âu ay ây eo êu ia iêu iu oai oao oay oeo oi ôi ơi ưa uây ui ưi uôi ươi ươu ưu uya uyu yêu",
    "ă",
    "i",
];

const LAST_CONSONANT_SEQS: &[&str] = &["ch nh", "c ng", "m n p t", "k", "c"];

const CV_MATRIX: &[&[usize]] = &[
    &[0, 1, 2, 5],
    &[0, 1, 2, 3, 4, 5],
    &[0, 1, 2, 3, 5],
    &[6],
    &[7],
];

const VC_MATRIX: &[&[usize]] = &[&[0, 2], &[0, 1, 2], &[1, 2], &[1, 2], &[], &[], &[3], &[4]];

/// CVC lookup (word tokens are space-separated within each row string).
fn lookup(
    seq: &[&str],
    input: &str,
    input_is_full: bool,
    input_is_complete: bool,
) -> Option<Vec<usize>> {
    let input_runes: Vec<char> = input.chars().collect();
    let input_len = input_runes.len();
    let mut ret = Vec::new();
    for (index, row) in seq.iter().enumerate() {
        let mut i = 0usize;
        let mut row_runes: Vec<char> = row.chars().collect();
        row_runes.push(' ');
        for (j, &ch) in row_runes.iter().enumerate() {
            if ch != ' ' {
                continue;
            }
            let canvas = &row_runes[i..j];
            i = j + 1;
            if canvas.len() < input_len || (input_is_full && canvas.len() > input_len) {
                continue;
            }
            let mut is_match = true;
            for (k, &ic) in input_runes.iter().enumerate() {
                let ck = canvas[k];
                // Go: ic != canvas[k] && !(!inputIsComplete && AddMarkToTonelessChar(canvas[k],0)==ic)
                if ic != ck && (input_is_complete || add_mark_to_toneless_char(ck, 0) != ic) {
                    is_match = false;
                    break;
                }
            }
            if is_match {
                ret.push(index);
                break;
            }
        }
    }
    if ret.is_empty() {
        None
    } else {
        Some(ret)
    }
}

fn is_valid_cv(fc_indexes: &[usize], vo_indexes: &[usize]) -> bool {
    for &fc in fc_indexes {
        if let Some(row) = CV_MATRIX.get(fc) {
            for &c in *row {
                for &vo in vo_indexes {
                    if c == vo {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn is_valid_vc(vo_indexes: &[usize], lc_indexes: &[usize]) -> bool {
    for &vo in vo_indexes {
        if let Some(row) = VC_MATRIX.get(vo) {
            for &c in *row {
                for &lc in lc_indexes {
                    if c == lc {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub(crate) fn is_valid_cvc(fc: &str, vo: &str, lc: &str, input_is_full_complete: bool) -> bool {
    let mut fc_indexes: Option<Vec<usize>> = None;
    let mut vo_indexes: Option<Vec<usize>> = None;
    let mut lc_indexes: Option<Vec<usize>> = None;

    if !fc.is_empty() {
        fc_indexes = lookup(
            FIRST_CONSONANT_SEQS,
            fc,
            input_is_full_complete || !vo.is_empty(),
            true,
        );
        if fc_indexes.is_none() {
            return false;
        }
    }
    if !vo.is_empty() {
        vo_indexes = lookup(
            VOWEL_SEQS,
            vo,
            input_is_full_complete || !lc.is_empty(),
            input_is_full_complete,
        );
        if vo_indexes.is_none() {
            return false;
        }
    }
    if !lc.is_empty() {
        lc_indexes = lookup(LAST_CONSONANT_SEQS, lc, input_is_full_complete, true);
        if lc_indexes.is_none() {
            return false;
        }
    }

    let vo_indexes = match vo_indexes {
        Some(v) => v,
        None => return fc_indexes.is_some(),
    };

    if let Some(fc_idx) = fc_indexes {
        let ret = is_valid_cv(&fc_idx, &vo_indexes);
        if !ret || lc_indexes.is_none() {
            return ret;
        }
    }
    match lc_indexes {
        Some(lc_idx) => is_valid_vc(&vo_indexes, &lc_idx),
        None => true,
    }
}
