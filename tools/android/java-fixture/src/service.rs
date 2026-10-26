//! Development-only foreground preparation fixture; no account or gameplay APIs.
use jni::{JNIEnv, objects::{JClass,JString}, sys::{jstring,jboolean}};
use std::{fs, path::Path, sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}, io::Write};
static JOB: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_horizon_javaprobe_PreparationService_nativeReset(_:JNIEnv,_:JClass)->jboolean {1}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_horizon_javaprobe_PreparationService_nativeCancel(_:JNIEnv,_:JClass){
 if let Some(c)=JOB.lock().unwrap().as_ref(){c.store(true,Ordering::Relaxed);}
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_horizon_javaprobe_PreparationService_nativePrepare(mut env:JNIEnv,_:JClass,external:JString,internal:JString,native:JString)->jstring {
 let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->Result<(),String>{
 let external:String=env.get_string(&external).map_err(|e|e.to_string())?.into();
 let internal:String=env.get_string(&internal).map_err(|e|e.to_string())?.into();
 let native:String=env.get_string(&native).map_err(|e|e.to_string())?.into();
 let cancel=Arc::new(AtomicBool::new(false));
 {let mut job=JOB.lock().map_err(|e|e.to_string())?;if job.is_some(){return Err("Another test is running".into())}*job=Some(cancel.clone());}
 let run=(||->Result<(),String>{
 let external=Path::new(&external);let internal=Path::new(&internal);
 let log=external.join("probe.log");fs::write(&log,"START full preparation in target35 app sandbox\n").map_err(|e|e.to_string())?;
 let progress=|s:&str|{if let Ok(mut f)=fs::OpenOptions::new().append(true).open(&log){let _=writeln!(f,"{s}");}};
 let home=internal.join("runtime");if !home.join("lib/modules").is_file(){super::copy_runtime(&external.join("runtime"),&home)?;}
 let root=internal.join("preparation");
 if !root.join(".inputs-copied").is_file(){
 progress("Copying isolated official inputs to private fixture storage");
 for name in ["minecraft","mods","research-lock.json","server.jar"] {
 let source=external.join("preparation").join(name); let target=root.join(name);
 if source.is_dir(){super::copy_runtime(&source,&target)?;}else{fs::create_dir_all(&root).map_err(|e|e.to_string())?;fs::copy(source,target).map_err(|e|e.to_string())?;}
 }
 fs::write(root.join(".inputs-copied"),b"ok").map_err(|e|e.to_string())?;
 }
 let helper=Path::new(&native).join("libhorizon_java.so");
 let jdk=horizon_prepare::java::Jdk::android(25,&home,&helper)?.with_cancel(cancel.clone());
 let options=horizon_prepare::Options{root:root.clone(),import:true,minecraft_dir:Some(root.join("minecraft")),server_jar:Some(root.join("server.jar")),java_home:None};
 let prepared=horizon_prepare::prepare_with_jdk(&options,jdk,&progress)?;
 horizon_prepare::sounds(&options.root,options.minecraft_dir.as_deref(),&progress)?;
 fs::copy(prepared.join("horizon-resources.json"),external.join("prepared-manifest.json")).map_err(|e|e.to_string())?;
 progress("HORIZON_FULL_ANDROID_PREPARATION_OK");Ok(())})();
 *JOB.lock().map_err(|e|e.to_string())?=None;run
 }));
 match result{Ok(Ok(()))=>std::ptr::null_mut(),other=>{let error=match other{Ok(Err(e))=>e,_=>"Preparation panic".into()};env.new_string(error).map_or(std::ptr::null_mut(),|s|s.into_raw())}}
}
