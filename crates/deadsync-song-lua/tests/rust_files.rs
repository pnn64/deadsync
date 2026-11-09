use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{SongLuaCompileContext, rust_engine::RustSongRuntime};
use std::{fs, path::Path};

struct Pair {
    rust: RustSongRuntime,
}
impl Pair {
    fn new(path: &Path) -> Self {
        reference_test::register_path(path);
        let context = SongLuaCompileContext::new(path, "file and font helper migration");
        let mut pair = Self {
            rust: RustSongRuntime::new(&context).unwrap(),
        };
        pair.compare(r#"
            function snapshot(v)
                if type(v)=='table' then
                    local entries={}
                    for k,value in pairs(v) do entries[#entries+1]=snapshot(k)..'='..snapshot(value) end
                    table.sort(entries);return '{'..table.concat(entries,';')..'}'
                elseif type(v)=='number' then return v~=v and 'nan' or string.format('%.17g',v)
                elseif type(v)=='string' then return 's'..#v..':'..v
                elseif type(v)=='boolean' then return tostring(v)
                else return type(v) end
            end
            function pack(...) return {n=select('#',...),...} end
            function observe(fn,...)
                local result=pack(pcall(fn,...))
                if not result[1] then return {ok=false} end
                return result
            end
            __songlua_side_effect_count=0
        "#);
        pair
    }
    fn compare(&mut self, source: &str) {
        let actual = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/files.lua").unwrap())
            .unwrap_or_else(|error| panic!("Rust: {source}\n{error:?}"))
            .to_vec();
        let actual = if actual.is_empty() {
            None
        } else {
            assert_eq!(actual.len(), 1, "{source}");
            let Value::String(id) = actual[0] else {
                panic!("string expected: {source}")
            };
            Some(reference_test::snapshot_bytes(
                self.rust.vm_mut().string_bytes(id).unwrap(),
            ))
        };
        reference_test::assert_source_debug(
            "song/tests/rust_files.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

fn directory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("files")).unwrap();
    fs::create_dir(dir.path().join("files/Zdir")).unwrap();
    fs::create_dir(dir.path().join("files/adir")).unwrap();
    fs::write(dir.path().join("files/b.txt"), b"abc\0\n").unwrap();
    fs::write(dir.path().join("files/A.txt"), b"123").unwrap();
    fs::write(dir.path().join("files/zero.dat"), b"").unwrap();
    dir
}

#[test]
fn rage_file_read_write_seek_eof_and_destroy_match_the_existing_contract() {
    let dir = directory();
    fs::write(dir.path().join("input.bin"), b"alpha\r\n\nbeta\rZ\0tail").unwrap();
    let mut p = Pair::new(dir.path());
    p.compare(
        r#"
        local file = RageFileUtil.CreateRageFile()
        assert(not file:Open('missing.txt', 1) and #file:GetError() > 0)
        assert(not pcall(function() file:GetLine() end))
        assert(file:Flush() == file and file:GetError() == 'Not open')
        assert(not file:Open('input.bin', 0))
        assert(file:GetError() == 'Neither reading nor writing specified')
        assert(not file:Open('input.bin', 3))
        assert(file:GetError() == 'Reading and writing are mutually exclusive')
        assert(file:Open('input.bin', 1))
        assert(file:GetLine() == 'alpha' and file:Tell() == 7 and not file:AtEOF())
        assert(file:GetLine() == '' and file:Tell() == 8 and not file:AtEOF())
        assert(file:GetLine() == 'beta\rZ' and file:Tell() == 19 and not file:AtEOF())
        assert(file:GetLine() == '' and file:AtEOF())
        assert(file:Seek(file:Tell()) == 19 and file:AtEOF())
        assert(file:Seek(0) == 0 and not file:AtEOF())
        assert(file:ReadBytes(7) == 'alpha\r\n' and file:Tell() == 7)
        assert(file:ReadBytes(0) == '' and not file:AtEOF())
        assert(file:Read() == '\nbeta\rZ' and file:AtEOF())
        assert(not pcall(function() file:Write('x') end))
        assert(file:Flush() == file)
        assert(file:ClearError() == file.ClearError)
        assert(file:Close() == file.Close)
        assert(file:Open('output.txt', 2))
        assert(file:Write('abc') == 3 and file:PutLine('def') == 2)
        assert(file:Flush() == file)
        assert(not pcall(function() file:Read() end))
        file:Close()
        local reader = RageFileUtil.CreateRageFile()
        assert(reader:Open('output.txt', 1) and reader:Read() == 'abcdef\r\n')
        reader:Close()
        assert(file:destroy() == file.destroy)
        assert(not pcall(function() file:GetLine() end))
        return snapshot{__songlua_side_effect_count}
    "#,
    );
    assert!(!dir.path().join("output.txt").exists());
}

#[test]
fn rage_file_argument_conversion_return_arity_and_error_recovery_match() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.compare(r#"
        local file=RageFileUtil:CreateRageFile(false,{},function()error('ignored')end)
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack('file'),pack({}),pack(file),
            pack(file,false,1),pack(file,{},1),pack(file,'files/A.txt'),
            pack(file,'files/A.txt',false),pack(file,'files/A.txt',1.9),
            pack(file,'files/A.txt',' 1.9 '),pack(file,'files/A.txt','0x1'),
            pack(file,'files/A.txt',0/0),pack(file,'files/A.txt',math.huge),
            pack(file,'files/A.txt',2147483648),pack(file,'files/A.txt',-2147483649)} do
            result[#result+1]=observe(file.Open,unpack(args,1,args.n))
        end
        for _,args in ipairs{pack(),pack(nil),pack(false),pack({}),pack(0),pack(1.9),pack('1.9'),
            pack(-1),pack(-2),pack('0x2'),pack(2147483648),pack(-2147483649),pack(0/0),pack(math.huge)} do
            assert(file:Open('files/b.txt',1))
            result[#result+1]=observe(file.ReadBytes,file,unpack(args,1,args.n))
            result[#result+1]=pack(file:Tell(),file:AtEOF())
            result[#result+1]=observe(file.Seek,file,unpack(args,1,args.n))
            result[#result+1]=pack(file:Tell(),file:AtEOF())
        end
        assert(file:Open('virtual.txt',2))
        for _,args in ipairs{pack(),pack(nil),pack(false),pack({}),pack(12.5),pack('a\0ignored')} do
            result[#result+1]=observe(file.Write,file,unpack(args,1,args.n))
            result[#result+1]=observe(file.PutLine,file,unpack(args,1,args.n))
        end
        assert(file:Close('first','last')=='last')
        assert(file:ClearError(nil)==nil)
        local marker={}
        assert(file:destroy(marker)==marker)
        for _,name in ipairs{'Open','Write','Read','GetLine','ReadBytes','Seek','Tell','AtEOF','GetError','Flush','Close','ClearError','destroy'} do
            result[#result+1]=observe(file[name],file,'ignored',1)
        end
        return snapshot{result,__songlua_side_effect_count}
    "#);
    assert!(!dir.path().join("virtual.txt").exists());
}

#[test]
fn rage_file_saved_methods_gc_reentry_and_session_write_isolation_match() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.compare(
        r#"
        local create=RageFileUtil.CreateRageFile
        RageFileUtil=nil;collectgarbage('collect')
        local writer=create();assert(writer:Open('virtual.txt',2))
        assert(writer:Write('hello')==5);writer:Flush()
        local reader=create();assert(not reader:Open('virtual.txt',1))
        local close=writer.Close;writer.Close=nil
        local reads,writes=0,0
        rawset(_G,'__songlua_side_effect_count',nil)
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_side_effect_count' then
                reads=reads+1;collectgarbage('collect')
                assert(not pcall(writer.Tell,writer));return 10
            end
        end,__newindex=function(t,k,v)
            if k=='__songlua_side_effect_count' then
                writes=writes+1;assert(not pcall(close,writer));return
            end
            rawset(t,k,v)
        end})
        assert(not pcall(close,writer))
        setmetatable(_G,nil);rawset(_G,'__songlua_side_effect_count',0)
        assert(reads==1 and writes==1)
        local open,read=reader.Open,reader.Read
        reader=nil;writer=nil;collectgarbage('collect');collectgarbage('collect')
        assert(open({},'virtual.txt',1));assert(read({})=='hello')
        assert(open({},'files/A.txt',1));assert(read({})=='123')
        return snapshot{reads,writes,__songlua_side_effect_count}
    "#,
    );
    assert!(!dir.path().join("virtual.txt").exists());
    let mut next = Pair::new(dir.path());
    next.compare("return snapshot{RageFileUtil.CreateRageFile():Open('virtual.txt',1)}");
}

#[test]
fn file_queries_preserve_paths_sorting_filters_argument_rules_and_errors() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.compare(r#"
        assert(table.concat(FILEMAN:GetDirListing('files'),',')=='A.txt,Zdir,adir,b.txt,zero.dat')
        local results={}
        for _,path in ipairs{'files',' files/ ','files/*.txt','files/?.txt','files/b.txt','missing','missing/*.txt','files/b.txt/*'} do
            for _,flags in ipairs{pack(),pack(true),pack(false,true),pack('YES','1'),pack('no','off'),pack('NaN'),pack({}),pack('\255')} do
                results[#results+1]=observe(FILEMAN.GetDirListing,{},path,unpack(flags,1,flags.n))
            end
            results[#results+1]=pack(FILEMAN.DoesFileExist(path),FILEMAN:GetFileSizeBytes(path),FILEMAN:GetHashForFile(path))
        end
        for _,args in ipairs{pack(),pack(nil),pack(false),pack(123),pack({}),pack({},nil),pack('\255'),pack('files\0suffix')} do
            for _,name in ipairs{'GetDirListing','DoesFileExist','GetFileSizeBytes','GetHashForFile'} do
                results[#results+1]=observe(FILEMAN[name],unpack(args,1,args.n))
            end
        end
        assert(FILEMAN:GetDirListing('missing')~=FILEMAN:GetDirListing('missing'))
        assert(FILEMAN:DoesFileExist('files') and FILEMAN:GetFileSizeBytes('files')==0)
        assert(FILEMAN:GetFileSizeBytes('files/b.txt')==5)
        return snapshot{results,__songlua_side_effect_count}
    "#);
}

#[test]
fn mutation_stubs_keep_result_arity_counts_and_files_unchanged() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.compare(
        r#"
        local result={}
        for _,name in ipairs{'Copy','CreateDir','Remove','Unzip','FlushDirCache'} do
            local fn=FILEMAN[name]
            result[#result+1]=pack(fn('files/b.txt','copied.txt'))
            result[#result+1]=pack(fn({},'files/b.txt'))
            result[#result+1]=pack(fn())
        end
        assert(__songlua_side_effect_count==15)
        assert(FILEMAN:DoesFileExist('files/b.txt') and not FILEMAN:DoesFileExist('copied.txt'))
        return snapshot{result,__songlua_side_effect_count}
    "#,
    );
    assert_eq!(
        fs::read(dir.path().join("files/b.txt")).unwrap(),
        b"abc\0\n"
    );
    assert!(!dir.path().join("copied.txt").exists());
}

#[test]
fn saved_file_callbacks_survive_collection_and_effects_allow_global_reentry() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.compare(r#"
        local listing=FILEMAN.GetDirListing;local exists=FILEMAN.DoesFileExist
        local remove=FILEMAN.Remove;local flush=FILEMAN.FlushDirCache
        FILEMAN=nil;collectgarbage('collect')
        assert(exists('files/b.txt'));local result={listing('files',true)}
        for _,count in ipairs{0,1.9,-2.9,'3.9','0x10','bad',false,{},0/0,1/0,9223372036854775808} do
            __songlua_side_effect_count=count
            result[#result+1]={observe(remove),__songlua_side_effect_count}
        end
        rawset(_G,'__songlua_side_effect_count',nil)
        local gets,sets=0,0
        local old=getmetatable(_G)
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_side_effect_count' then
                gets=gets+1;collectgarbage('collect');assert(exists('files/b.txt'));return '8.9'
            end
        end,__newindex=function(t,k,v)
            if k=='__songlua_side_effect_count' then
                sets=sets+1;collectgarbage('collect');assert(#listing('files')==5);error('record failed')
            end
            rawset(t,k,v)
        end})
        result[#result+1]=observe(flush)
        setmetatable(_G,old);assert(gets==1 and sets==1)
        result[#result+1]={gets,sets};return snapshot(result)
    "#);
}

#[test]
fn sprite_hint_stripping_preserves_native_string_conversion_and_suffix_rules() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack({}),pack(123),pack(1.5),
            pack('Wendy 10x2.png'),pack('Wendy (doubleres) 10x2.PNG'),pack('x 1x2.jpg'),
            pack('x 1x2.png suffix'),pack('x 1x2.png\0'),pack('x 1X2.png'),pack('x (doubleres).png'),
            pack('\255.png'),pack('café 2x2.png')} do
            result[#result+1]=observe(StripSpriteHints,unpack(args,1,args.n))
        end
        return snapshot(result)
    "#);
}

#[test]
fn combo_fonts_scan_current_theme_and_preserve_wendy_order_without_fallbacks() {
    let dir = directory();
    let fonts = dir.path().join("theme/Fonts/_Combo Fonts");
    for name in [
        "Alpha",
        "Wendy",
        "Wendy (Cursed)",
        "Zebra",
        "NoPng",
        "NoIni",
        "NoFrame",
        "Jpeg",
    ] {
        let path = fonts.join(name);
        fs::create_dir_all(&path).unwrap();
        if name != "NoIni" {
            fs::write(path.join(format!("{name}.ini")), b"[main]").unwrap();
        }
        if name != "NoPng" {
            let suffix = match name {
                "NoFrame" => ".png",
                "Jpeg" => " 10x2.jpg",
                _ => " 10x2.png",
            };
            fs::write(path.join(format!("{name}{suffix}")), b"fixture").unwrap();
        }
    }
    let mut p = Pair::new(dir.path());
    p.compare(
        r#"
        assert(#GetComboFonts()==0)
        THEME.GetCurrentThemeDirectory=function()return 'theme/' end
        local first=GetComboFonts();local second=GetComboFonts()
        assert(first~=second and table.concat(first,',')=='Wendy,Alpha,Zebra,Wendy (Cursed)')
        THEME.GetCurrentThemeDirectory=function()return 'missing/' end
        assert(#GetComboFonts()==0)
        return snapshot{first,second,__songlua_side_effect_count}
    "#,
    );
}

#[test]
fn combo_font_helper_resolves_current_lua_callbacks_during_collection_and_reentry() {
    let dir = directory();
    let mut p = Pair::new(dir.path());
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                b"function inspect_source(f) return debug.getinfo(f,'S').source end",
                b"=test source inspector",
            )
            .unwrap(),
        )
        .unwrap();
    p.compare(
        r#"
        local helper=GetComboFonts;local actual=FILEMAN.GetDirListing
        local calls={};local nested=false
        THEME={GetCurrentThemeDirectory=function(self)
            assert(self==THEME);calls[#calls+1]='theme';return 'virtual/'
        end}
        FILEMAN={GetDirListing=function(self,path,dirs,full)
            assert(self==FILEMAN);collectgarbage('collect');assert(#actual('files')==5)
            calls[#calls+1]=path..':'..tostring(dirs)..':'..tostring(full)
            if dirs then return {'Zebra','Wendy','Wendy (Cursed)'} end
            local name=path:match('/([^/]+)/$');return {name..' 2x2.png',name..'.ini'}
        end}
        local original=StripSpriteHints
        StripSpriteHints=function(name)
            collectgarbage('collect')
            if not nested then nested=true;assert(#helper()==3) end
            return original(name)
        end
        GetComboFonts=nil;collectgarbage('collect')
        local fonts=helper();assert(table.concat(fonts,',')=='Wendy,Zebra,Wendy (Cursed)')
        assert(inspect_source(helper)=='Simply Love/Scripts/SL-Helpers.lua:GetComboFonts')
        FILEMAN.GetDirListing=function()error('listing failed')end
        assert(not pcall(helper))
        return snapshot{fonts,calls}
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
