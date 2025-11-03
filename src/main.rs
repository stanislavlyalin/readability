use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use regex::Regex;

/// CLI: вычисление "понятности" текста по частотному словарю из английской Википедии.
#[derive(Parser, Debug)]
#[command(name = "readability", version, about)]
struct Args {
    /// Путь к JSON-словарю вида [["the", 199660765], ...]
    #[arg(long = "dict", default_value = "word_frequencies.json")]
    dict_path: PathBuf,

    /// Путь к текстовому файлу для оценки; если не указан — читаем текст из STDIN
    #[arg(long = "text")]
    text_path: Option<PathBuf>,

    /// Анализировать только первые N слов входного текста (по порядку в тексте)
    #[arg(long = "top-text-words")]
    top_text_words: Option<usize>,

    /// Использовать только первые K записей словаря (ускорение/эксперименты)
    #[arg(long = "top-dict-entries")]
    top_dict_entries: Option<usize>,
}

fn load_frequency_dict(path: &PathBuf, top_k: Option<usize>) -> Result<HashMap<String, f64>> {
    let mut f = File::open(path)
        .with_context(|| format!("Не удалось открыть словарь: {}", path.display()))?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let json: serde_json::Value = serde_json::from_str(&s)
        .with_context(|| "Некорректный JSON частотного словаря")?;

    let arr = json.as_array().context("Ожидался JSON-массив верхнего уровня")?;

    // Разбираем пары ["word", count]
    let mut items: Vec<(String, u64)> = Vec::with_capacity(arr.len());
    for v in arr {
        if let Some(a) = v.as_array() {
            if a.len() >= 2 {
                let word = a[0]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Первый элемент не строка"))?
                    .to_string();
                let count = a[1]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Второй элемент не число"))?;
                items.push((word, count));
            } else {
                bail!("Элемент массива словаря имеет длину < 2");
            }
        } else {
            bail!("Элемент словаря не является массивом из двух значений");
        }
    }

    if let Some(k) = top_k {
        items.truncate(k.min(items.len()));
    }

    if items.is_empty() {
        bail!("Словарь пуст");
    }

    let max_count = items.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let max_count_f = max_count as f64;

    let mut map = HashMap::with_capacity(items.len());
    for (w, c) in items {
        let weight = (c as f64) / max_count_f; // в [0,1], максимум=1.0
        map.insert(w, weight);
    }
    Ok(map)
}

fn read_input_text(path: &Option<PathBuf>) -> Result<String> {
    let mut buf = String::new();
    match path {
        Some(p) => {
            let mut f = File::open(p)
                .with_context(|| format!("Не удалось открыть входной текст: {}", p.display()))?;
            f.read_to_string(&mut buf)?;
        }
        None => {
            io::stdin()
                .read_to_string(&mut buf)
                .context("Не удалось прочитать текст из STDIN")?;
        }
    }
    Ok(buf)
}

fn tokenize_english_words(text: &str) -> Vec<String> {
    // Слова: последовательности латинских букв; апострофы внутри слов допускаем (can't, I'm)
    // Всё в нижнем регистре
    let re = Regex::new(r"[A-Za-z]+(?:'[A-Za-z]+)?").unwrap();
    re.find_iter(text)
        .map(|m| m.as_str().to_ascii_lowercase())
        .collect()
}

fn compute_readability(
    tokens: &[String],
    dict_weights: &HashMap<String, f64>,
    top_text_words: Option<usize>,
) -> Option<f64> {
    let iter = tokens.iter();
    let iter = if let Some(n) = top_text_words {
        Box::new(iter.take(n)) as Box<dyn Iterator<Item = &String>>
    } else {
        Box::new(iter) as Box<dyn Iterator<Item = &String>>
    };

    let mut sum = 0.0f64;
    let mut cnt = 0usize;

    for w in iter {
        let wgt = dict_weights.get(w).copied().unwrap_or(0.0);
        sum += wgt;
        cnt += 1;
    }

    if cnt == 0 { None } else { Some(sum / (cnt as f64)) }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let dict = load_frequency_dict(&args.dict_path, args.top_dict_entries)?;
    let text = read_input_text(&args.text_path)?;
    let tokens = tokenize_english_words(&text);

    let score = compute_readability(&tokens, &dict, args.top_text_words)
        .ok_or_else(|| anyhow::anyhow!("Не найдено ни одного слова для оценки"))?;

    // Печатаем только число — удобно для пайпов и автоматизации
    println!("{:.6}", score);

    Ok(())
}
