use std::{collections::HashMap, io::Read};

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Config {
    pub areas: Vec<Area>,
}

#[derive(serde::Deserialize, Debug)]
struct Area {
    name: String,
    pub admin: String,
    pub db: String,
}

impl Area {
    fn def(&self) -> proc_macro2::TokenStream {
        self.pascal_case_name().parse().unwrap()
    }

    fn pascal_case_name(&self) -> String {
        self.name.replace(' ', "")
    }

    fn normal_name(&self) -> &str {
        &self.name
    }
}

#[proc_macro_attribute]
pub fn area_list(attr: TokenStream, item: TokenStream) -> TokenStream {
    let config_source_path = parse_macro_input!(attr as syn::LitStr).value();
    let item = parse_macro_input!(item as syn::Item);

    let item = match item {
        syn::Item::Enum(e) => e,
        _ => {
            panic!("macro can only be applied to an enum");
        }
    };

    let areas = match areas(config_source_path) {
        Ok(a) => a,
        Err(e) => panic!("{}", e),
    };

    let ident = item.ident;
    let vis = item.vis;
    let attrs = item.attrs;

    let area_defs = areas.iter().map(|a| a.def());

    let try_from_pats = areas.iter().map(|a| {
        let normal_name = a.normal_name();
        let ty = format!("Self::{}", a.pascal_case_name())
            .parse::<proc_macro2::TokenStream>()
            .unwrap();
        quote! { #normal_name => ::std::result::Result::Ok(#ty) }
    });

    let mut admins: HashMap<&String, Vec<String>> = HashMap::new();
    for area in &areas {
        match admins.get_mut(&area.admin) {
            Some(v) => v.push(area.pascal_case_name()),
            None => {
                admins.insert(&area.admin, vec![area.pascal_case_name()]);
            }
        }
    }

    let admin_of_pats = admins
        .iter()
        .map(|(admin, areas)| -> proc_macro2::TokenStream {
            let mut areas_vec = "::smallvec::smallvec![".to_owned();
            for area in areas {
                areas_vec.push_str(&format!("Self::{},", area));
            }
            areas_vec.push(']');
            format!(r#""{}" => {}"#, admin, areas_vec).parse().unwrap()
        });

    quote! {
        #(#attrs)*
        #vis enum #ident {
            #(#area_defs),*
        }

        impl ::std::convert::TryFrom<&str> for #ident {
            type Error = ();

            fn try_from(s: &str) -> ::std::result::Result<Self, Self::Error> {
                match s {
                    #(#try_from_pats),*,
                    _ => ::std::result::Result::Err(())
                }
            }
        }

        impl #ident {
            pub fn admin_of<S>(id: S) -> ::smallvec::SmallVec<[#ident; 1]>
            where
                S: AsRef<str>
            {
                match id.as_ref() {
                    #(#admin_of_pats),*,
                    _ => ::smallvec::smallvec![]
                }
            }
        }
    }
    .into()
}

fn areas(path: String) -> Result<Vec<Area>, &'static str> {
    let mut contents = String::new();
    let mut file = std::fs::File::open(&path).map_err(|_| "error opening config file")?;
    file.read_to_string(&mut contents)
        .map_err(|_| "error reading config file")?;
    toml::from_str::<Config>(&contents)
        .map(|c| c.areas)
        .map_err(|_| "error parsing config file")
}
