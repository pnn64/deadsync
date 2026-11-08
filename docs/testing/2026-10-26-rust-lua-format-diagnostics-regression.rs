
use std::fmt::Write;
macro_rules! probe {($lua:ident,$engine:expr,$name:expr,$source:expr)=>{{
    let p=$lua::compile_named($source.as_bytes(),b"=format-probe").unwrap();
    let mut vm=$lua::Vm::new();vm.set_output_capture(true);vm.execute(&p).unwrap();
    let mut hex=String::new();for byte in vm.output().unwrap(){write!(&mut hex,"{byte:02x}").unwrap();}
    println!("result\t{}\t{}\t{}\t{}\t{}",$engine,$name,p.instruction_count(),p.register_count(),hex);
}}}
fn main(){
    println!("size\tparent\t{}\t{}",std::mem::size_of::<lua_parent::Vm>(),std::mem::size_of::<lua_parent::Value>());
    println!("size\tcurrent\t{}\t{}",std::mem::size_of::<lua_current::Vm>(),std::mem::size_of::<lua_current::Value>());
    for(name,source)in [("trailing_percent","local ok,e=pcall(string.format,'%',17);local t={};for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))"),
("trailing_flag","local ok,e=pcall(string.format,'%0',17);local t={};for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))"),
("nul_conversion","local ok,e=pcall(string.format,'%\\000tail',17);local t={};for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))"),
("precision_nul","local ok,e=pcall(string.format,'prefix%.\\000tail',17);local t={};for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))"),
("invalid_nonzero","local ok,e=pcall(string.format,'%Z',17);local t={};for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))"),
("literal_nul","local ok,e=pcall(string.format,'before\\000after',17);local t={};for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))")]{probe!(lua_parent,"parent",name,source);probe!(lua_current,"current",name,source);}
}
