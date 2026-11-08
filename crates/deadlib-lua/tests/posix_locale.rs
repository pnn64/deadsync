#![cfg(all(target_os = "linux", target_env = "gnu"))]
use deadlib_lua::{Value, Vm, compile};
use std::sync::Mutex;

// Locale selection is shared between VMs, just as in standalone Lua. Keep
// independent expectations serialized; the last test creates its own readers.
static TEST_LOCALE: Mutex<()> = Mutex::new(());

fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}
fn utf8(vm: &mut Vm) -> Vec<u8> {
    let values = run(
        vm,
        br#"
      assert(os.setlocale('C'))
      local name=os.setlocale('C.UTF-8','ctype') or os.setlocale('C.utf8','ctype')
      assert(name,'C.UTF-8 is required by this platform regression')
      return name
    "#,
    );
    let Value::String(name) = values[0] else {
        panic!("locale name must be a string")
    };
    vm.string_bytes(name).unwrap().to_vec()
}

#[test]
fn c_locale_queries_failures_nul_names_dates_and_native_error_bytes() {
    let _guard = TEST_LOCALE.lock().unwrap();
    assert_eq!(
        run(
            &mut Vm::new(),
            br#"
      assert(os.setlocale('C')=='C')
      for _,category in ipairs{'all','collate','ctype','monetary','numeric','time'} do
        assert(os.setlocale('POSIX',category)=='C')
        assert(os.setlocale(nil,category)=='C')
      end
      assert(os.setlocale('deadlua-invalid-locale-name')==nil and os.setlocale()=='C')
      assert(os.setlocale('C\0ignored','numeric\0ignored')=='C')
      assert(not pcall(os.setlocale,'C','missing-category'))
      assert(os.date('!%A|%B|%Y-%m-%d',0)=='Thursday|January|1970-01-01')
      local f,message,code=io.open('','r')
      assert(f==nil and message==': No such file or directory' and code==2)
      return true
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn categories_composite_restore_and_selection_are_shared_between_vms() {
    let _guard = TEST_LOCALE.lock().unwrap();
    let mut first = Vm::new();
    let name = utf8(&mut first);
    let mut second = Vm::new();
    let name = second.intern(&name);
    second.set_global(b"chosen", name).unwrap();
    assert_eq!(
        run(
            &mut second,
            br#"
      assert(os.setlocale(nil,'ctype')==chosen and os.setlocale(nil,'numeric')=='C')
      local mixed=os.setlocale()
      assert(mixed:find('LC_CTYPE=',1,true) and mixed:find('LC_NUMERIC=C',1,true))
      assert(os.setlocale('C')=='C')
      assert(os.setlocale(mixed)==mixed and os.setlocale(nil,'ctype')==chosen)
      assert(os.setlocale('deadlua-invalid-locale-name','ctype')==nil)
      assert(os.setlocale()==mixed)
      return true
    "#
        ),
        [Value::Boolean(true)]
    );
    run(&mut first, b"assert(os.setlocale('C')=='C')");
    assert_eq!(
        run(&mut second, b"return os.setlocale()=='C'"),
        [Value::Boolean(true)]
    );
}

#[test]
fn replaced_locale_objects_survive_parallel_native_formatting_and_comparison() {
    let _guard = TEST_LOCALE.lock().unwrap();
    let mut vm = Vm::new();
    let name = utf8(&mut vm);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let name = name.clone();
            std::thread::spawn(move || {
                let mut vm = Vm::new();
                let name = vm.intern(&name);
                vm.set_global(b"chosen", name).unwrap();
                assert_eq!(
                    run(
                        &mut vm,
                        br#"
              for i=1,80 do
                assert(os.setlocale(i%2==0 and 'C' or chosen))
                assert(os.date('!%Y-%m-%d',0)=='1970-01-01')
                assert(string.upper('aZ')=='AZ' and tostring(1.5)=='1.5')
                local t={'z','a\0c','a\0b'};table.sort(t)
                assert(t[1]=='a\0b' and t[2]=='a\0c' and t[3]=='z')
              end
              return true
            "#
                    ),
                    [Value::Boolean(true)]
                );
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    run(&mut vm, b"assert(os.setlocale('C')=='C')");
}
