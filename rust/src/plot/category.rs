use std::borrow::BorrowMut;
use std::collections::HashMap;

use crate::utils::format_si;

#[derive(Clone, Debug)]
pub struct Category {
    pub title: String,
    pub total: bool,
    pub members: Vec<String>,
    pub indices: Vec<usize>,
    pub color: String,
    pub count: Option<usize>,
    pub span: Option<usize>,
    pub n50: Option<usize>,
}

impl Default for Category {
    fn default() -> Category {
        Category {
            title: "".to_string(),
            total: false,
            members: vec![],
            indices: vec![],
            color: "#999999".to_string(),
            count: None,
            span: None,
            n50: None,
        }
    }
}

impl Category {
    pub fn subtitle(self) -> String {
        let mut parts = vec![];
        if self.count.is_some() {
            parts.push(format_si(&(self.count.unwrap() as f64), 3, None))
        }
        if self.span.is_some() {
            parts.push(format_si(&(self.span.unwrap() as f64), 3, None))
        }
        if self.n50.is_some() {
            parts.push(format_si(&(self.n50.unwrap() as f64), 3, None))
        }
        if !parts.is_empty() {
            return format!("[{}]", parts.join("; "));
        }
        "".to_string()
    }
}

pub fn set_cat_order(
    values: &Vec<(String, usize)>,
    z_values: &Vec<f64>,
    order: &Option<String>,
    count: &usize,
    palette: &Vec<String>,
) -> (Vec<Category>, Vec<Option<usize>>) {
    let mut indices = HashMap::new();
    let mut title_list = vec![];
    for (i, entry) in values.iter().enumerate() {
        title_list.push(entry.clone().0);
        if !indices.contains_key(&entry.0) {
            indices.insert(entry.clone().0, vec![i]);
        } else {
            let list = indices.get_mut(&entry.0).unwrap();
            list.push(i);
        }
    }
    let frequencies = values
        .iter()
        .enumerate()
        .map(|(i, x)| (x.clone().0, i))
        .fold(HashMap::new(), |mut map, (val, i)| {
            map.entry(val)
                .and_modify(|(frq, span)| {
                    *frq += 1;
                    *span += z_values[i]
                })
                .or_insert((1, z_values[i]));
            map
        });
    let mut sorted_cats: Vec<_> = frequencies.clone().into_iter().collect();
    sorted_cats.sort_by(|x, y| {
        if (x.1).0 == (y.1).0 {
            if (x.1).1 == (y.1).1 {
                x.0.partial_cmp(&y.0).unwrap()
            } else {
                (y.1).1.partial_cmp(&(x.1).1).unwrap()
            }
        } else {
            (y.1).0.cmp(&(x.1).0)
        }
    });

    let mut cat_order = vec![];
    let mut all_indices: Vec<usize> = vec![];
    let mut all_members: Vec<String> = vec![];
    let mut index = 0;
    if order.is_some() {
        // TODO: prevent duplication when adding remaining cats
        for entry in order.clone().unwrap().split(",") {
            if frequencies.contains_key(entry) {
                let title = entry.to_string();
                cat_order.push(Category {
                    title: entry.to_string(),
                    members: vec![title.clone()],
                    indices: indices[&title].clone(),
                    color: palette[index].clone(),
                    ..Default::default()
                });
            }
            index += 1;
        }
    }
    for (title, _) in &sorted_cats {
        all_indices.append(indices.clone().get_mut(title).unwrap());
        all_members.push(title.clone());
        if cat_order.iter().any(|cat| cat.title == title.clone()) {
            continue;
        }
        if index < count - 1 || index == count - 1 && *count == sorted_cats.len() {
            cat_order.push(Category {
                title: title.clone(),
                members: vec![title.clone()],
                indices: indices[title].clone(),
                color: palette[index].clone(),
                ..Default::default()
            });
            index += 1
        } else if cat_order.len() < *count {
            cat_order.push(Category {
                title: "other".to_string(),
                members: vec![title.clone()],
                indices: indices[title].clone(),
                color: palette[count - 1].clone(),
                ..Default::default()
            });
        } else {
            let other_cat = cat_order[count - 1].borrow_mut();
            other_cat.members.push(title.clone());
            other_cat.indices.append(indices.get_mut(title).unwrap());
        }
    }
    cat_order.insert(
        0,
        Category {
            title: "total".to_string(),
            total: true,
            indices: all_indices,
            members: all_members,
            ..Default::default()
        },
    );
    let mut cat_indices: Vec<Option<usize>> = (0..values.len()).map(|x| Some(x)).collect();
    for (index, cat) in cat_order.iter_mut().enumerate() {
        // use this loop for span, count and n50
        let mut lengths = vec![];
        for i in cat.indices.iter() {
            cat_indices[*i] = Some(index);
            lengths.push(z_values[*i]);
        }
        lengths.sort_by(|a, b| b.partial_cmp(a).unwrap());
        cat.count = Some(cat.indices.len());
        let span = lengths.iter().sum::<f64>();
        let mut cumulative = 0.0;
        for length in lengths.iter() {
            cumulative += length;
            if cumulative >= span / 2.0 {
                cat.n50 = Some(length.clone() as usize);
                break;
            }
        }
        cat.span = Some(lengths.iter().sum::<f64>() as usize);
    }
    (cat_order, cat_indices)
}
