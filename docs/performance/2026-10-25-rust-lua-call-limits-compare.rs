use std::{hint::black_box,time::Instant};
fn main(){
 println!("sizes\tparent_error={}\tcurrent_error={}\tparent_vm={}\tcurrent_vm={}",std::mem::size_of::<lua_parent::Error>(),std::mem::size_of::<lua_current::Error>(),std::mem::size_of::<lua_parent::Vm>(),std::mem::size_of::<lua_current::Vm>());
 for (name,source,repeats) in [("for",b"local sum=0;for i=1,100000 do sum=sum+i end;return sum".as_slice(),20),("while",b"local i,sum=0,0;while i<100000 do i=i+1;sum=sum+i end;return sum",20),("fib",b"local function f(n)if n<2 then return n end;return f(n-1)+f(n-2)end;return f(20)",10)] {
  let p=lua_parent::compile(source).unwrap();let c=lua_current::compile(source).unwrap();let mut pv=lua_parent::Vm::new();let mut cv=lua_current::Vm::new();
  let expected=if name=="fib" {6765.0}else{5000050000.0};assert_eq!(pv.execute(&p).unwrap(),&[lua_parent::Value::Number(expected)]);assert_eq!(cv.execute(&c).unwrap(),&[lua_current::Value::Number(expected)]);
  for _ in 0..5 {black_box(pv.execute(&p).unwrap());black_box(cv.execute(&c).unwrap());}
  for sample in 0..9 {for engine in if sample%2==0 {[0,1]}else{[1,0]} {let now=Instant::now();for _ in 0..repeats {if engine==0 {black_box(pv.execute(black_box(&p)).unwrap());}else{black_box(cv.execute(black_box(&c)).unwrap());}}println!("{name}\t{engine}\t{sample}\t{}",now.elapsed().as_nanos()/repeats);}}
 }
}
