use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::BufReader;
use walkdir::WalkDir;

#[derive(Debug)]
struct BobError {
    text: String,
}

impl fmt::Display for BobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.text)
    }
}

impl Error for BobError {}

fn main() {
    let mut args: Vec<String> = env::args().collect();
    //println!("Args are -> {:?}", args);

    if args.contains(&String::from("help")) {
        print_helptext();
        return;
    }

    let mut flags = define_flags();
    let flags = set_flags(&mut args, &mut flags);

    //can't use log_type after moved into log_factory
    let log_type = String::from(flags["log"]);
    let log = log_factory(log_type);

    if !has_required(&flags) {
        log(&format!("{}", "There is no term or regexp defined! Example: file_crawler term=\"find me\" or file_crawler regexp=^startswith"));
        return;
    }

    let search_term: Regex;
    let result = set_search_term(&flags, &log);
    match result {
        Ok(v) => search_term = v,
        Err(_) => return,
    }
    //println!("{:?}", search_term);

    let ext = flags["ext"];
    let extensions: Vec<Regex>;
    let result = extensions_factory(ext);
    match result {
        Ok(v) => extensions = v,
        Err(e) => {
            log(&format!("{}", e.text));
            return;
        }
    }
    //println!("{:?}", extensions);

    let root = flags["root"];
    search(&search_term, &root, &extensions, &log);
}

fn define_flags<'a>() -> HashMap<&'a str, &'a str> {
    let mut flags = HashMap::new();
    flags.insert("term", ""); //The simple alphanumeric-only term you're searching for. Example: term="find me"
    flags.insert("root", "./"); //The starting folder for searching. Example root="c:/Looky Here"
    flags.insert("ext", "txt"); //Up to 25 file extension(s) to search. Example -ext=txt,doc
    flags.insert("case", "n"); //y for case sensitive
    flags.insert("regexp", ""); //will search by regexp instead of term and case. Example regexp=(?i)^startswith
    flags.insert("log", "console"); //Where to log names of files containing the search. Can use "console". Example: -log=C:/Logs/Log.txt or -log=console
    flags
}

fn print_helptext() {
    let help_text = r#"
    You must set either term or regexp
    The simple alphanumeric term you're searching for. Example: term="find me". Default is ""
    The starting folder for searching. Example root="c:/Looky Here". Default is ./
    Up to 25 file extension(s) to search. Example -ext=txt,doc. Default is txt
    y for case sensitive. Default is n
    Will search by regexp instead of term and case. Example regexp=(?i)^startswith. Default is ""
    Where to log names of files containing the search. Example: -log=C:/Logs/Log.txt Default is console
    "#;
    print!("{}", help_text);
}

fn set_flags<'a>(
    args: &'a mut Vec<String>,
    flags: &'a mut HashMap<&'a str, &'a str>,
) -> HashMap<&'a str, &'a str> {
    args.retain(|flag| flag.contains('='));
    //println!("Filtered args are -> {:?}", args);
    for arg in args.iter() {
        let setting: Vec<&str> = arg.splitn(2, '=').collect();
        //println!("setting is -> {:?}", setting);
        if setting.len() < 2 {
            continue;
        }
        if flags.contains_key(setting[0]) {
            flags.insert(setting[0], setting[1]);
        }
    }
    //println!("flags are -> {:?}", flags);
    //println!("Filtered args are -> {:?}", args);
    //we're done changing flags, so return immutable HashMap
    flags.clone()
}

fn has_required(flags: &HashMap<&str, &str>) -> bool {
    flags["term"] != "" || flags["regexp"] != ""
}

fn set_search_term(
    flags: &HashMap<&str, &str>,
    logger: &dyn Fn(&str),
) -> Result<Regex, regex::Error> {
    let term = flags["term"];
    let case = flags["case"];
    let regexp = flags["regexp"];
    let result = set_regex(&regexp, &case, &term);
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            print_regerror(&e, &regexp, &case, &term, &logger);
            Err(e)
        }
    }
}

fn set_regex(regexp: &str, case: &str, term: &str) -> Result<Regex, regex::Error> {
    if regexp != "" {
        Regex::new(regexp)
    } else {
        let caseterm = if case == "y" || case == "Y" {
            ""
        } else {
            "(?i)"
        };
        let mut searchterm = String::from(caseterm);
        searchterm.push_str(term);
        Regex::new(&searchterm)
    }
}

fn print_regerror(
    error: &regex::Error,
    regexp: &str,
    case: &str,
    term: &str,
    logger: &dyn Fn(&str),
) {
    if regexp != "" {
        logger(&format!("Problem regexp {} into regex {:?}", regexp, error));
    } else {
        logger(&format!(
            "Problem parsing term \"{}\" and case \"{}\" into regex {:?}",
            term, case, error
        ));
    }
}

fn extensions_factory(ext: &str) -> Result<Vec<Regex>, BobError> {
    //do case insensitive match for filename ending. Example: "(?i)\.txt$"
    //maximum of 25 extensions
    let raw_extensions: Vec<&str> = ext.splitn(26, ',').collect();
    if raw_extensions.len() > 25 {
        return Err(BobError {
            text: "Surpassed 25 extensions".to_string(),
        });
    }

    let mut regexts: Vec<Regex> = Vec::with_capacity(raw_extensions.len());
    for raw_extension in raw_extensions.iter() {
        let mut raw_extension = raw_extension.to_string();
        if raw_extension.starts_with(".") {
            raw_extension = "(?i)\\".to_string() + &raw_extension + "$";
        } else {
            raw_extension = "(?i)\\.".to_string() + &raw_extension + "$";
        }
        let reg_result = Regex::new(&raw_extension);
        match reg_result {
            Err(_) => {
                return Err(BobError {
                    text: format!("Failed to accept extension {}", &raw_extension),
                })
            }
            Ok(v) => regexts.push(v),
        }
    }

    Ok(regexts.clone())
}

fn search(search_reg: &Regex, root: &str, extensions: &Vec<Regex>, logger: &dyn Fn(&str)) {
    for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if !is_valid_file(entry.file_name().to_str().unwrap(), extensions) {
                continue;
            }
            if file_has_match(&entry, &search_reg) {
                logger(&format!("{}", entry.path().to_str().unwrap()));
            }
        }
    }
}

fn is_valid_file(file_name: &str, extensions: &Vec<Regex>) -> bool {
    for extension in extensions {
        if extension.is_match(file_name) {
            return true;
        }
    }
    false
}

fn file_has_match(entry: &walkdir::DirEntry, search_reg: &Regex) -> bool {
    let file: File;
    let result = File::open(entry.path());
    match result {
        Ok(v) => file = v,
        Err(_) => return false,
    }
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    let result = buf_reader.read_to_string(&mut contents);
    match result {
        Ok(_) => search_reg.is_match(&contents),
        Err(_) => false,
    }
}

//log_name String is moved into log_factory so a reference to it can be used in OpenOptions.open.
//The file appender function needs "move" so it can own the log_file value,
//otherwise it causes lifetime issues.
fn log_factory<'a>(log_name: String) -> Box<dyn Fn(&str)> {
    if log_name == "console" {
        return Box::new(|msg: &str| print!("{}", msg));
    } else {
        return Box::new(move |msg: &str| {
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&log_name)
                .unwrap();
            file.write_all(msg.as_bytes()).unwrap();
            file.write_all("\n".as_bytes()).unwrap();
        });
    }
}
