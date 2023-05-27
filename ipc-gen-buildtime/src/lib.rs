use quote::{format_ident, quote};
use serde_derive::Deserialize;
use std::env;
use std::fs;
use std::path::Path;
use syn::{Path as SynPath, Type};

#[derive(Debug, Deserialize)]
struct Ty {
    name: String,
    ty: String,
}

#[derive(Debug, Deserialize)]
struct Method {
    name: String,
    id: u32,
    inputs: Vec<Ty>,
    output: String,
    is_async: Option<bool>,
}

impl Method {
    fn server(&self) -> syn::__private::TokenStream2 {
        let input_names: Vec<_> = self
            .inputs
            .iter()
            .map(|x| format_ident!("{}", x.name))
            .collect();
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .map(|x| {
                let name = format_ident!("{}", x.name);
                let ty_: Type = syn::parse_str(&x.ty).unwrap();
                quote!(#name: #ty_)
            })
            .collect();

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

        quote! {
            #method_id => {
                request_msg.read_translates();

                #(let #inputs = request_msg.read();)*

                tokio::spawn(async move {
                    let res: #output_type = self.#method_name (#(#input_names),*) #maybe_await;
                    let mut reply_msg = unsafe { process::ipc::message::IPCMessage::new(&mut IPC_BUFFER) };
                    reply_msg.write(res);
                    reply_msg.write_translates();
                    reply_msg.write_header_for(0);

                    unsafe { crate::syscalls::ipc_reply(h, &mut IPC_BUFFER).unwrap(); }
                });
            }
        }
    }

    fn client(&self, handle_accessor: &str) -> syn::__private::TokenStream2 {
        let ipc_handle_accessor: SynPath = syn::parse_str(handle_accessor).unwrap();

        let input_names: Vec<_> = self
            .inputs
            .iter()
            .map(|x| format_ident!("{}", x.name))
            .collect();
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .map(|x| {
                let name = format_ident!("{}", x.name);
                let ty_: Type = syn::parse_str(&x.ty).unwrap();
                quote!(#name: #ty_)
            })
            .collect();

        let output_type: Type = syn::parse_str(&self.output).unwrap();
        let dispatch_output = if self.output == "()" {
            quote! {}
        } else {
            quote! {
                let out: #output_type = reply_msg.read();
                out
            }
        };

        let write_inputs = if input_names.len() == 0 {
            quote! {}
        } else {
            quote! { #(request_msg.write(#input_names));*; }
        };

        let method_name = format_ident!("{}", self.name);
        let method_id: u32 = self.id;

        quote! {
            pub fn #method_name ( #(#inputs),* ) -> #output_type {
                let h = #ipc_handle_accessor();
                let mut request_msg = unsafe { crate::ipc::message::IPCMessage::new(&mut IPC_BUFFER) };

                #write_inputs

                request_msg.write_header_for(#method_id);
                request_msg.write_translates();

                unsafe { crate::syscalls::ipc_request(h, &mut IPC_BUFFER).unwrap(); }

                let mut reply_msg = unsafe { crate::ipc::message::IPCMessage::new(&mut IPC_BUFFER) };
                reply_msg.read_header();
                reply_msg.read_translates();

                #dispatch_output
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    name: String,
    struct_name: String,
    handle_accessor: String,
    main_interface: Interface,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    sub_interfaces: Vec<Interface>
}

#[derive(Debug, Deserialize)]
struct Interface {
    name: Option<String>,
    session_name: String,
    methods: Vec<Method>
}

fn generate_server_ipcserver_impl(server: &ServerConfig) -> String {
    let server_struct_name = format_ident!("{}", server.struct_name);
    let server_session_enum_name = format_ident!("{}SessionEnum", server.struct_name);
    let server_session_wrapper_name = format_ident!("{}SessionWrapper", server.struct_name);

    let mut all_subinterface_names: Vec<String> = Vec::new();
    all_subinterface_names.push(server.main_interface.session_name.clone());
    all_subinterface_names.extend(server.sub_interfaces.iter().map(|x| x.session_name.clone()));

    let main_interface_ident = format_ident!("{}", server.main_interface.session_name);

    let mut all_subinterface_idents: Vec<_> = all_subinterface_names.iter().map(|x| format_ident!("{}", x)).collect();

    let server_impl = quote!(
        enum #server_session_enum_name {
            #(#all_subinterface_idents(Arc<#all_subinterface_idents>),)*
        }

        #(impl #all_subinterface_idents {
            fn get_server(&self) -> Arc<#server_struct_name> {
                self.__server.clone()
            }
        })*

        impl IPCSession for #server_session_enum_name {
            fn process(self :Arc<Self>, h: Handle, ipc_buffer: &mut [u8]) {
                match self.as_ref() {
                    #(#server_session_enum_name::#all_subinterface_idents(x) => {
                        x.clone().process(h, ipc_buffer);
                    })*
                }
            }
        }

        impl IPCServer for #server_struct_name {
            fn get_server_impl<'a>(self: &'a Arc<Self>) -> MutexGuard<'a, ServerImpl> {
                self.__server_impl.lock().unwrap()
            }

            fn accept_main_session_in_trait(self: &Arc<Self>) -> Arc<dyn IPCSession> {
                Arc::<#server_session_enum_name>::new(self.accept_session().into())
            }
        }

        #(
        impl From<Arc<#all_subinterface_idents>> for #server_session_enum_name {
            fn from(x: Arc<#all_subinterface_idents>) -> Self {
                #server_session_enum_name::#all_subinterface_idents(x)
            }
        })*
    );
    server_impl.to_string()
}

fn generate_server_interface(interface: &Interface) -> String {
    let server_methods: Vec<_> = interface.methods.iter().map(|x| x.server()).collect();
    let session_name = format_ident!("{}", interface.session_name);

    let server_impl = quote!(
        impl IPCSession for #session_name {
            fn process(self: std::sync::Arc<Self>, h: Handle, ipc_buffer: &mut [u8]) {
                self.process_internal(h, ipc_buffer);
            }
        }

        impl #session_name {
            fn process_internal(self: std::sync::Arc<Self>, h: Handle, ipc_buffer: &mut [u8]) {
                let mut request_msg = process::ipc::message::IPCMessage::new(ipc_buffer);
                request_msg.read_header();

                match request_msg.header.id {
                    #(#server_methods),*,
                    _ => { panic!("Unexpected IPC message ID!") }
                }
            }
        }
    );
    server_impl.to_string()
}

pub fn generate_server(path: &str) {
    let spec = toml::from_str::<ServerConfig>(&fs::read_to_string(path).unwrap()).unwrap();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(spec.name.clone() + "_server_impl.rs");

    let header = "use process::ipc::message::IPC_BUFFER;\n
    use process::ipc_server::*;
    use std::sync::MutexGuard;".to_string();

    let server_impl = generate_server_ipcserver_impl(&spec);
    let server_main_impl = generate_server_interface(&spec.main_interface);
    //let server_sub_impl = spec.sub_interfaces.iter().map(|x| generate_server_interface(x)).collect::<Vec<String>>().join("\n");
    fs::write(dest_path, header + &server_impl + &server_main_impl).unwrap();

    println!("cargo:rerun-if-changed={}", path);
}

pub fn generate_client(path: &str) {
    let spec = toml::from_str::<ServerConfig>(&fs::read_to_string(path).unwrap()).unwrap();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(spec.name + "_client_impl.rs");

    let client_methods: Vec<_> = spec.main_interface
        .methods
        .iter()
        .map(|x| x.client(&spec.handle_accessor))
        .collect();

    let client_impl = quote! {
        use crate::ipc::message::IPC_BUFFER;

        #(#client_methods)*
    };

    fs::write(dest_path, client_impl.to_string()).unwrap();
    println!("cargo:rerun-if-changed={}", path);
}
