extern crate viper_sys;
extern crate jni;
extern crate env_logger;
extern crate error_chain;

use std::fs;
use std::convert::From;
use jni::JavaVM;
use jni::InitArgsBuilder;
use jni::JNIVersion;
use jni::objects::JObject;
use error_chain::ChainedError;
use viper_sys::java::*;
use viper_sys::scala::*;
use viper_sys::jvm::*;
use viper_sys::verifier::*;
use viper_sys::viper_ast::*;

#[test]
fn test_call_silicon() {
    env_logger::init().expect("failed to initialize env_logger");

    let jar_paths: Vec<String> = fs::read_dir("/usr/lib/viper/")
        .unwrap()
        .map(|x| x.unwrap().path().to_str().unwrap().to_owned())
        .collect();

    let jvm_args = InitArgsBuilder::new()
        .version(JNIVersion::V8)
        .option(&format!("-Djava.class.path={}", jar_paths.join(":")))
        .option("-Xdebug")
        .build()
        .unwrap_or_else(|e| {
            panic!(format!("{}", e.display_chain().to_string()));
        });

    let jvm = JavaVM::new(jvm_args).unwrap_or_else(|e| {
        panic!(format!("{}", e.display_chain().to_string()));
    });


    let env = jvm.attach_current_thread().expect(
        "failed to attach jvm thread",
    );

    env.with_local_frame(16, || {
        let silicon = new_silicon(&env)?;

        let silicon_args_array = JObject::from(
            env.new_object_array(3, "java/lang/String", JObject::null())?,
        );

        env.set_object_array_element(
            silicon_args_array.into_inner(),
            0,
            From::from(env.new_string("--z3Exe")?),
        )?;

        env.set_object_array_element(
            silicon_args_array.into_inner(),
            1,
            From::from(env.new_string("/usr/local/Viper/z3/bin/z3")?),
        )?;

        env.set_object_array_element(
            silicon_args_array.into_inner(),
            2,
            From::from(env.new_string("dummy-program.sil")?),
        )?;

        let scala_predef = get_predef(&env)?;

        let silicon_args_seq = wrap_ref_array(&env, scala_predef, silicon_args_array)?;

        parse_command_line(&env, silicon, silicon_args_seq)?;

        start(&env, silicon)?;

        reset(&env, silicon)?;

        let program = new_program(
            &env,
            new_mutable_array_seq(&env, 0)?,
            new_mutable_array_seq(&env, 0)?,
            new_mutable_array_seq(&env, 0)?,
            new_mutable_array_seq(&env, 0)?,
            new_mutable_array_seq(&env, 0)?,
            get_no_position(&env)?,
            get_no_info(&env)?,
            get_no_trafos(&env)?,
        )?;

        let verification_result = verify(&env, silicon, program)?;

        let system_out = get_system_out(&env)?;

        println_object(&env, system_out, verification_result)?;

        stop(&env, silicon)?;

        Ok(JObject::null())

    }).unwrap_or_else(|e| {
            print_exception(&env);
            panic!(format!("{}", e.display_chain().to_string()));
        });
}
