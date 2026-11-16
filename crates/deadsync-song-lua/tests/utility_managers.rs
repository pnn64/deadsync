use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{SongLuaCompileContext, SongRuntime};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "utility manager migration")
}

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/utility-managers.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}

struct Pair {
    rust: SongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        let mut pair = Self {
            rust: SongRuntime::new(context).unwrap(),
        };
        pair.compare(HELPERS);
        pair
    }
    fn compare(&mut self, source: &str) {
        // Each probe returns an explicit, deterministic serialization including
        // result arity. Native error wrappers are compared by success and state.
        let values = execute(&mut self.rust, source);
        let actual = if values.is_empty() {
            None
        } else {
            assert_eq!(values.len(), 1, "{source}");
            let Value::String(id) = values[0] else {
                panic!("serialized result expected: {source}");
            };
            Some(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
        };
        reference_test::assert_source_debug(
            "song/tests/rust_utility_managers.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

const HELPERS: &str = r#"
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
function observe(owner,fn,...)
    local result=pack(pcall(fn,...))
    if not result[1] then return {ok=false} end
    for i=2,result.n do if result[i]==owner then result[i]='owner' end end
    return result
end
"#;

#[test]
fn trace_and_warn_keep_argument_rules_counter_effects_and_reentry() {
    let context = context();
    let mut pair = Pair::new(&context);
    pair.compare(r#"
        local trace,warn=Trace,Warn
        Trace=nil;Warn=nil;collectgarbage('collect')
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack({}),pack(function()error('ignored')end),
            pack(12.5),pack('text'),pack('\255'),pack('a\0b',{},function()error('ignored')end)} do
            result[#result+1]=observe(nil,trace,unpack(args,1,args.n))
            result[#result+1]=observe(nil,warn,unpack(args,1,args.n))
        end
        result[#result+1]=__songlua_side_effect_count
        for _,count in ipairs{pack(),pack(1.9),pack(-2.9),pack('3.9'),pack('0x10'),
            pack('bad'),pack(false),pack({}),pack(0/0),pack(math.huge)} do
            rawset(_G,'__songlua_side_effect_count',count[1])
            result[#result+1]={observe(nil,warn),__songlua_side_effect_count}
        end
        local reads,writes=0,0
        rawset(_G,'__songlua_side_effect_count',nil)
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_side_effect_count' then
                reads=reads+1;collectgarbage('collect');if reads==1 then warn({}) end;return 4
            end
        end,__newindex=function(t,k,v)
            if k=='__songlua_side_effect_count' then writes=writes+1;return end
            rawset(t,k,v)
        end})
        warn(nil,false,{})
        setmetatable(_G,nil)
        assert(reads==2 and writes==2)
        result[#result+1]={reads,writes}
        return snapshot(result)
    "#);
}

#[test]
fn product_identity_and_ignored_arguments_match_production() {
    for live in [false, true] {
        let mut context = context();
        context.live = live;
        let mut pair = Pair::new(&context);
        pair.compare(
            r#"
            local saved = {ProductFamily, ProductID, ProductVersion}
            assert(#saved == 3)
            ProductFamily, ProductID, ProductVersion = nil, nil, nil
            collectgarbage('collect')
            local result = {}
            for i, product in ipairs(saved) do
                result[i] = {}
                for j, args in ipairs{pack(), pack(nil), pack(false), pack({}, 1, 'ignored', function() error('ignored') end)} do
                    result[i][j] = pack(product(unpack(args, 1, args.n)))
                end
            end
            return snapshot(result)
        "#,
        );
    }
}

#[test]
fn complete_manager_surfaces_and_ignored_arguments_match_production() {
    for live in [false, true] {
        let mut context = context();
        context.live = live;
        context.style_name = "pump-double".into();
        let mut p = Pair::new(&context);
        p.compare(
            r#"
            local results={}
            for _,name in ipairs{'GAMEMAN','CHARMAN','MEMCARDMAN','UNLOCKMAN','HOOKS'} do
                local owner=_G[name];local names={};local result={}
                for key in pairs(owner) do names[#names+1]=key end
                table.sort(names)
                for _,key in ipairs(names) do
                    local fn=owner[key];assert(type(fn)=='function')
                    result[key]={}
                    for i,args in ipairs{pack(),pack(nil),pack(owner),pack(owner,nil,false),
                        pack({},'pump',-2,true),pack('dance',1,{},function()error('unused')end)} do
                        result[key][i]=observe(owner,fn,unpack(args,1,args.n))
                    end
                end
                results[name]=result
            end
            return snapshot{results,__songlua_side_effect_count}
        "#,
        );
    }
}

#[test]
fn game_style_metadata_and_column_argument_rules_match_production() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        local style=GAMEMAN:GetStylesForGame('pump')[1]
        assert(style:GetName()=='single' and style:ColumnsPerPlayer()==4)
        local original={style:GetName(),style:GetStepsType(),style:GetStyleType(),style:GetWidth()}
        style.__songlua_style_key='pump-double';style.ColumnsPerPlayer=function()return 100 end
        local result={original}
        for _,args in ipairs{pack(),pack(style),pack(style,'PlayerNumber_P1'),
            pack(style,'PlayerNumber_P2',1),pack(style,0,0),pack(style,0,-5),
            pack(style,0,2.5),pack(style,0,' 3.5 '),pack(style,0,99),
            pack(style,0,'nan'),pack(style,0,'inf'),pack(style,0,0/0),
            pack(style,0,math.huge),pack(style,0,{}),pack(style,0,true),
            pack(style,0,'bad'),pack(0,3),pack(nil,3),pack({},0,2)} do
            result[#result+1]=observe(style,style.GetColumnInfo,unpack(args,1,args.n))
        end
        return snapshot(result)
    "#,
    );
}

#[test]
fn lists_styles_and_multi_result_queries_return_fresh_tables() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local a,b=GAMEMAN:GetStylesForGame(),GAMEMAN:GetStylesForGame()
        assert(a~=b and a[1]~=b[1] and a[1].GetName~=b[1].GetName)
        local first,second=a[1]:GetColumnInfo(0,1),a[1]:GetColumnInfo(0,1)
        assert(first~=second);first.Name='mutated';a[1].GetName=function()return 'mutated'end
        local chars1,chars2=CHARMAN:GetAllCharacters(),CHARMAN:GetAllCharacters()
        local songs1,songs2=UNLOCKMAN:GetSongsUnlockedByEntryID(),UNLOCKMAN:GetSongsUnlockedByEntryID()
        local steps1,owners1=UNLOCKMAN:GetStepsUnlockedByEntryID()
        local steps2,owners2=UNLOCKMAN:GetStepsUnlockedByEntryID()
        assert(chars1~=chars2 and songs1~=songs2 and steps1~=owners1 and steps1~=steps2 and owners1~=owners2)
        chars1[1]=true;songs1[1]=true;steps1[1]=true;owners1[1]=true
        collectgarbage('collect')
        return snapshot{second,b[1]:GetName(),chars2,songs2,steps2,owners2,
            select('#',CHARMAN:GetCharacter()),select('#',UNLOCKMAN:FindEntryID()),
            select('#',UNLOCKMAN:GetStepsUnlockedByEntryID())}
    "#);
}

#[test]
fn side_effect_counters_errors_and_chaining_keep_the_original_owner() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local original=UNLOCKMAN;local methods={}
        for _,name in ipairs{'PreferUnlockEntryID','UnlockEntryID','UnlockEntryIndex','LockEntryID','LockEntryIndex'} do
            methods[#methods+1]=original[name]
        end
        UNLOCKMAN={};tonumber=function()error('replacement primitive')end
        local result={}
        for _,count in ipairs{pack(),pack(0),pack(-1),pack(2.75),pack('12'),pack('0x10'),
            pack(false),pack('bad'),pack({})} do
            rawset(_G,'__songlua_side_effect_count',count[1])
            local row={}
            for i,fn in ipairs(methods) do row[i]=observe(original,fn,{},123,false) end
            row.count=__songlua_side_effect_count;result[#result+1]=row
        end
        __songlua_side_effect_count=0
        assert(HOOKS:SetClipboard('test')==false and HOOKS:GetClipboard()=='')
        assert(MEMCARDMAN:MountCard(0)==false and MEMCARDMAN:GetCardState()=='MemoryCardState_none')
        result[#result+1]=__songlua_side_effect_count
        return snapshot(result)
    "#);
}

#[test]
fn counter_callbacks_preserve_reentry_gc_error_order_and_retained_results() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local original=UNLOCKMAN;local saved=original.UnlockEntryID
        local trace={};rawset(_G,'__songlua_side_effect_count',nil)
        local reads=0
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_side_effect_count' then
                reads=reads+1;collectgarbage('collect');trace[#trace+1]='read'
                if reads==1 then assert(MEMCARDMAN:MountCard()==false) end
                return '4'
            end
        end,__newindex=function(t,k,v)
            if k=='__songlua_side_effect_count' then
                collectgarbage('collect');trace[#trace+1]='write:'..v
                UNLOCKMAN={};return
            end
            rawset(t,k,v)
        end})
        assert(saved()==original)
        setmetatable(_G,nil)
        local result={table.concat(trace,'|')}
        for _,failure in ipairs{'read','write'} do
            trace={};rawset(_G,'__songlua_side_effect_count',nil)
            setmetatable(_G,{__index=function(t,k)
                if k=='__songlua_side_effect_count' then
                    trace[#trace+1]='read';if failure=='read' then error('counter read') end
                    return 7
                end
            end,__newindex=function(t,k,v)
                if k=='__songlua_side_effect_count' then trace[#trace+1]='write:'..v;error('counter write') end
                rawset(t,k,v)
            end})
            local ok=pcall(saved)
            setmetatable(_G,nil)
            result[#result+1]={ok,table.concat(trace,'|')}
        end
        return snapshot(result)
    "#);
}

#[test]
fn saved_methods_retain_only_their_required_owners_and_runtimes_are_isolated() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    execute(
        &mut runtime,
        r#"
        weak=setmetatable({},{__mode='v'})
        weak.game=GAMEMAN;weak.characters=CHARMAN;weak.card=MEMCARDMAN;weak.hooks=HOOKS
        styles=GAMEMAN.GetStylesForGame;chars=CHARMAN.GetAllCharacters
        mount=MEMCARDMAN.MountCard;open=HOOKS.OpenURL
        GAMEMAN=nil;CHARMAN=nil;MEMCARDMAN=nil;HOOKS=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(weak.game==nil and weak.characters==nil and weak.card==nil and weak.hooks==nil)
        assert(#styles()==1 and #chars()==0 and mount()==false and open()==false)
        local style=styles()[1];weak.style=style;column=style.GetColumnInfo;style=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(weak.style==nil and column(0,3).Track==2)
        weak.unlock=UNLOCKMAN;saved=UNLOCKMAN.UnlockEntryID;UNLOCKMAN=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(weak.unlock~=nil and saved()==weak.unlock)
        saved=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak.unlock==nil and __songlua_side_effect_count==3)
    "#,
    );
    let mut other = SongRuntime::new(&context()).unwrap();
    execute(
        &mut other,
        r#"
        assert(GAMEMAN:GetStylesForGame()[1]:GetName()=='single')
        assert(UNLOCKMAN:UnlockEntryID()==UNLOCKMAN and __songlua_side_effect_count==1)
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
