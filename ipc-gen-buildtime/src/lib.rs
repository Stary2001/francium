use serde_derive::Deserialize;
use std::env;
use std::fs;
use std::path::Path;
use quote::{quote, format_ident};
use syn::Type;

#[derive(Debug, Deserialize)]
struct Ty {
    name: String,
    ty: String
}

#[derive(Debug, Deserialize)]
struct Method {
    name: String,
    id: u32,
    inputs: Vec<Ty>,
    output: String,
    is_async: Option<bool>
}

impl Method {
    fn server(&self) -> syn::__private::TokenStream2 {
        let input_names: Vec<_> = self.inputs.iter().map(|x| format_ident!("{}", x.name)).collect();
        let inputs: Vec<_> = self.inputs.iter().map(|x| {
            let name = format_ident!("{}", x.name);
            let ty_ = format_ident!("{}", x.ty);
            quote!(#name: #ty_)
        }).collect();

        let method_name = format_ident!("{}", self.name);
        let method_id: u32 = self.id;

        let output_type: Type = syn::parse_str(&self.output).unwrap();

        let maybe_await = if let Some(is_async) = self.is_async {
            if is_async {
                quote!(.await)
            } else {
                quote!()
            }
        } else {
            quote!()
        };

        quote!{
            #method_id => {
                request_msg.read_translates();

                #(let #inputs = request_msg.read();)*

                let res: #output_type = self.#method_name (#(#input_names),*) #maybe_await;
                let mut reply_msg = process::ipc::message::IPCMessage::new();
                reply_msg.write(res);
                reply_msg.write_translates();
                reply_msg.write_header_for(0);

                unsafe { crate::syscalls::ipc_reply(h, &mut IPC_BUFFER).unwrap(); }
            }
        }
    }

    fn client(&self) -> syn::__private::TokenStream2 {
        quote!{}
    }
}
#[derive(Debug, Deserialize)]
struct ServerConfig {
    name: String,
    struct_name: String,
    methods: Vec<Method>
}

pub fn generate_server(path: &str) {
    let spec = toml::from_str::<ServerConfig>(&fs::read_to_string(path).unwrap()).unwrap();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(spec.name + "_server_impl.rs");

    let server_methods: Vec<_> = spec.methods.iter().map(|x| x.server()).collect();
    let server_struct_name = format_ident!("{}", spec.struct_name);

    let server_impl = quote!(
        use process::ipc::message::IPC_BUFFER;

        #[async_trait::async_trait]
        impl IPCServer for #server_struct_name {
            async fn process(&mut self, h: Handle) {
                self.process(h).await
            }
        }

        impl #server_struct_name {
            async fn process(&mut self, h: Handle) {
                let mut request_msg = process::ipc::message::IPCMessage::new();
                request_msg.read_header();

                match request_msg.header.id {
                    #(#server_methods),*,
                    _ => { panic!("Unexpected IPC message ID!") }
                }
            }
        }
    );

    fs::write(dest_path, server_impl.to_string()).unwrap();

    println!("cargo:rerun-if-changed={}", path);
}

pub fn generate_client(path: &str) {
    let spec = toml::from_str::<ServerConfig>(&fs::read_to_string(path).unwrap()).unwrap();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(spec.name + "_client_impl.rs");

    let client_methods: Vec<_> = spec.methods.iter().map(|x| x.client()).collect();

    let client_impl = quote!(
        #(#client_methods)*
    );

    fs::write(dest_path, client_impl.to_string()).unwrap();
    println!("cargo:rerun-if-changed={}", path);
}