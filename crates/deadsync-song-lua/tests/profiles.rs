use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{SongLuaCompileContext, SongRuntime};
use std::path::Path;

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn number(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/profiles.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
}
impl Pair {
    fn new() -> Self {
        let mut rust = SongRuntime::new(&SongLuaCompileContext::new(
            Path::new("."),
            "profiles migration",
        ))
        .unwrap();
        execute(&mut rust, "__songlua_side_effect_count=0");
        Self { rust }
    }
    fn compare(&mut self, source: &str) {
        let actual: Vec<_> = execute(&mut self.rust, source)
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => number(n),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                v => panic!("scalar expected {v:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_profiles.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

const VALUES: &[&str] = &[
    "nil",
    "true",
    "false",
    "0",
    "-0",
    "17",
    "-1.25",
    "0/0",
    "1/0",
    "'12.25'",
    "'0x10'",
    "'NaN'",
    "'inf'",
    "' yes '",
    "'OFF'",
    "'  text  '",
    "string.char(255)",
    "{}",
    "function()end",
];

#[test]
fn complete_profile_surface_defaults_and_slot_identities_match_production() {
    let mut p = Pair::new();
    p.compare(r#"
        local function keys(t)local r={};for k in pairs(t)do r[#r+1]=k end;table.sort(r);return table.concat(r,',')end
        profile=PROFILEMAN:GetProfile(0)
        return keys(PROFILEMAN),keys(profile),profile:GetDisplayName(),profile:GetLastUsedHighScoreName(),
            profile:GetType(),profile:GetGUID(),profile:GetProfileDir(),profile:GetIsMale(),profile:GetGoalType(),
            profile:GetIgnoreStepCountCalories(),profile:GetDisplayTotalCaloriesBurned(),profile:GetCharacter()
    "#);
    for value in VALUES.iter().copied().chain([
        "1",
        "2",
        "'P1'",
        "'p2'",
        "'PlayerNumber_P2'",
        "'ProfileSlot_Player1'",
        "'ProfileSlot_Player2'",
        "'ProfileSlot_Machine'",
        "'profileslot_machine'",
        "'Machine'",
    ]) {
        p.compare(&format!(r#"
            local m=PROFILEMAN;local a=m:GetProfile({value});local b=m.GetProfile({value})
            return a:GetDisplayName(),b:GetDisplayName(),a==m:GetProfile({value}),a==m:GetMachineProfile(),
                a==m:GetProfile(0),a==m:GetProfile(1),m:GetProfileDir({value}),m:GetPlayerName({value}),m:LocalProfileIDToDir({value})
        "#));
    }
    p.compare(r#"
        local m=PROFILEMAN;local a=m:GetLocalProfileFromIndex(1);local b=m:GetLocalProfileFromIndex(false)
        local p1=m:GetProfile(0);local p2=m:GetProfile(1);local machine=m:GetMachineProfile()
        assert(a==b and a~=p1 and p1~=p2 and p1~=machine and p1:GetUserTable()~=p2:GetUserTable())
        local z=m:GetLocalProfileIDs();z[1]='modified'
        assert(#m:GetLocalProfileIDs()==0 and m:GetLocalProfileDisplayNames()~=m:GetLocalProfileDisplayNames())
        return a:GetDisplayName(),m:GetNumLocalProfiles(),m:GetLocalProfileIDFromIndex(),m:GetLocalProfile(),
            m:GetLocalProfileIndexFromID(),m:IsPersistentProfile(),m:IsSongNew(),m:ProfileWasLoadedFromMemoryCard(),
            m:LastLoadWasTamperedOrCorrupt(),m:ProfileFromMemoryCardIsNew(),m:GetSongNumTimesPlayed()
    "#);
}

#[test]
fn profile_constant_methods_and_score_object_defaults_preserve_surface_and_identity() {
    let mut p = Pair::new();
    p.compare(r#"
        local function snapshot(t)
            local names={};for name,f in pairs(t)do if type(f)=='function'then names[#names+1]=name end end
            table.sort(names);local out={}
            for _,name in ipairs(names)do
                local ok,value=pcall(t[name],t)
                out[#out+1]=name..':'..tostring(ok)..':'..(type(value)=='table' and 'table' or tostring(value))
            end;return table.concat(out,'|')
        end
        local profile=PROFILEMAN:GetMachineProfile()
        local list=profile:GetHighScoreList();local scores=list:GetHighScores();local score=scores[1]
        assert(list~=profile:GetHighScoreList() and list~=profile:GetHighScoreListIfExists() and list~=profile:GetCategoryHighScoreList())
        assert(scores==list:GetHighScores() and score==list:GetHighScores()[1])
        local radar=score:GetRadarValues();assert(radar~=score:GetRadarValues())
        return snapshot(profile),snapshot(list),snapshot(score),radar:GetValue(),#profile:get_songs()
    "#);
}

#[test]
fn setters_keep_string_number_boolean_conversion_and_bound_receiver_rules() {
    let mut p = Pair::new();
    for (setter, getter, field) in [
        ("SetDisplayName", "GetDisplayName", "__songlua_display_name"),
        (
            "SetLastUsedHighScoreName",
            "GetLastUsedHighScoreName",
            "__songlua_last_score_name",
        ),
        (
            "SetWeightPounds",
            "GetWeightPounds",
            "__songlua_weight_pounds",
        ),
        ("SetVoomax", "GetVoomax", "__songlua_voomax"),
        ("SetBirthYear", "GetBirthYear", "__songlua_birth_year"),
        (
            "SetGoalCalories",
            "GetGoalCalories",
            "__songlua_goal_calories",
        ),
        ("SetGoalSeconds", "GetGoalSeconds", "__songlua_goal_seconds"),
        (
            "SetIgnoreStepCountCalories",
            "GetIgnoreStepCountCalories",
            "__songlua_ignore_step_count_calories",
        ),
        ("SetIsMale", "GetIsMale", "__songlua_is_male"),
    ] {
        for value in VALUES {
            p.compare(&format!(r#"
                local p=PROFILEMAN:GetProfile(0);local saved=p.{setter};local other={{}}
                assert(saved(other,{value},'ignored')==p and other.{field}==nil)
                local a,b=p.{field},p:{getter}();saved({value});return a,b,p:{getter}(),__songlua_side_effect_count
            "#));
        }
    }
}

#[test]
fn mutable_fields_keep_typed_getter_errors_character_values_and_user_table_identity() {
    let mut p = Pair::new();
    for (field, getter) in [
        ("__songlua_display_name", "GetDisplayName"),
        ("__songlua_total_calories", "GetTotalCaloriesBurned"),
        ("__songlua_is_male", "GetIsMale"),
        ("__songlua_user_table", "GetUserTable"),
    ] {
        for value in VALUES {
            p.compare(&format!(
                r#"
                local p=PROFILEMAN:GetProfile(0);p.{field}={value}
                local ok,v=pcall(p.{getter},p)
                if not ok then return false end
                if type(v)=='table' or type(v)=='function' then return true,type(v) end
                return true,v
            "#
            ));
        }
    }
    p.compare(r#"
        local p=PROFILEMAN:GetProfile(0);local t={value=17};p:SetCharacter(t)
        assert(p:GetCharacter()==t);p:SetGoalType(t);assert(p:GetGoalType()==t)
        p:SetGoalType();local absent=p:GetGoalType();p:SetGoalType(nil);local explicit=p:GetGoalType()
        p.__songlua_user_table=t;assert(p:GetUserTable()==t)
        p:SetLastUsedHighScoreName('Alice');local a=p:GetAllUsedHighScoreNames();a[1]='changed'
        local b=p:GetAllUsedHighScoreNames();assert(a~=b and b[1]=='Alice')
        p:SetLastUsedHighScoreName('');return absent,explicit,#p:GetAllUsedHighScoreNames(),p:GetCharacter().value
    "#);
}

#[test]
fn calories_preserve_float_order_display_and_partial_failures() {
    let mut p = Pair::new();
    for value in VALUES {
        p.compare(&format!(r#"
            local p=PROFILEMAN:GetProfile(0);p.__songlua_calories_today=0.1;p.__songlua_total_calories=16777216
            assert(p:AddCaloriesToDailyTotal({value})==p)
            return p:GetCaloriesBurnedToday(),p:GetTotalCaloriesBurned(),p:GetDisplayTotalCaloriesBurned(),__songlua_side_effect_count
        "#));
    }
    p.compare(r#"
        local p=PROFILEMAN:GetProfile(0);local events={};p.__songlua_calories_today=nil;p.__songlua_total_calories=nil
        setmetatable(p,{__index=function(t,k)
            events[#events+1]='read:'..k;collectgarbage('collect')
            if k=='__songlua_calories_today' then return 2 elseif k=='__songlua_total_calories' then return 3 end
        end,__newindex=function(t,k,v)
            events[#events+1]='write:'..k..':'..v;collectgarbage('collect')
            if k=='__songlua_total_calories' then error('total write failure')end;rawset(t,k,v)
        end})
        local count=__songlua_side_effect_count;local ok=pcall(p.AddCaloriesToDailyTotal,p,4)
        assert(not ok and count==__songlua_side_effect_count)
        local first=rawget(p,'__songlua_calories_today');setmetatable(p,nil)
        p.__songlua_calories_today=1;p.__songlua_total_calories=false
        local read=pcall(p.AddCaloriesToDailyTotal,p,4)
        return first,table.concat(events,'|'),read,p.__songlua_calories_today
    "#);
}

#[test]
fn setters_and_prefix_keep_metamethod_reentry_and_side_effect_write_order() {
    let mut p = Pair::new();
    p.compare(r#"
        local m=PROFILEMAN;local p=m:GetProfile(0);local save=p.SetDisplayName;local get=p.GetDisplayName
        p.__songlua_display_name=nil;local held={name='initial'};local events={}
        setmetatable(p,{__index=function(t,k)collectgarbage('collect');if k=='__songlua_display_name'then return held.name end end,
            __newindex=function(t,k,v)events[#events+1]=k..':'..v;held.name=v;collectgarbage('collect')end})
        assert(save({},'changed')==p);local name=get(false)
        m.__songlua_stats_prefix=nil
        setmetatable(m,{__index=function(t,k)if k=='__songlua_stats_prefix'then return 12 end end,
            __newindex=function(t,k,v)events[#events+1]=k..':'..v;rawset(t,k,v)end})
        local numeric=m:GetStatsPrefix();assert(m:SetStatsPrefix('custom')==m)
        local custom=m:GetStatsPrefix();m:SetStatsPrefix(17)
        local count=__songlua_side_effect_count;__songlua_side_effect_count={}
        local ok=pcall(save,p,'before failure');local written=held.name
        __songlua_side_effect_count=count
        return name,numeric,custom,m:GetStatsPrefix(),table.concat(events,'|'),ok,written,count
    "#);
    p.compare(r#"
        local m=PROFILEMAN;local p=m:GetProfile(0);tonumber=function()error('replacement')end
        p.__songlua_weight_pounds='0x10';__songlua_side_effect_count='10'
        local value=p:GetWeightPounds();local a=select('#',m:SaveMachineProfile());local b=m:SaveProfile();local c=m:SaveLocalProfile()
        assert(p:AddScreenshot()==p)
        return value,a,b,c,__songlua_side_effect_count
    "#);
}

#[test]
fn high_score_lookup_uses_raw_sequence_current_callbacks_and_reentrant_updates() {
    let mut p = Pair::new();
    p.compare(r#"
        list=PROFILEMAN:GetProfile(0):GetHighScoreList();scores=list:GetHighScores()
        assert(list:GetHighestScoreOfName('Machine')==scores[1] and list:GetRankOfName('Machine')==1)
        assert(list:GetHighestScoreOfName('machine')==nil and list:GetRankOfName(17)==0)
        scores[1]={GetName=function(self)
            scores[2]={GetName=function(self)collectgarbage('collect');return 17 end};collectgarbage('collect');return 'first'
        end}
        local rank=list:GetRankOfName('17');local selected=list:GetHighestScoreOfName('17')
        scores[1]=nil;setmetatable(scores,{__index=function()error('raw sequence required')end})
        local hole=list:GetRankOfName('17');rawset(scores,1,{GetName=function(self)
            scores[1]=nil;collectgarbage('collect');self.marker='kept';return 'selected'
        end})
        local kept=list:GetHighestScoreOfName('selected');collectgarbage('collect')
        return rank,selected:GetName(),hole,kept.marker
    "#);
    for value in [
        "false",
        "17",
        "{}",
        "function()return false end",
        "function()return string.char(255)end",
        "function()error('lookup failure')end",
    ] {
        p.compare(&format!("scores[1]={{GetName={value}}};return (pcall(list.GetRankOfName,list,'x')),(pcall(list.GetHighestScoreOfName,list,'x'))"));
    }
    p.compare("scores[1]=17;return (pcall(list.GetRankOfName,list,'x'))");
}

#[test]
fn retained_profile_and_score_callbacks_survive_global_replacement_and_collection() {
    let mut p = Pair::new();
    p.compare(r#"
        local manager=PROFILEMAN;local player=manager:GetProfile(0);player:SetDisplayName('saved')
        local query=player.GetDisplayName;local set=player.SetDisplayName
        local list=player:GetHighScoreList();local rank=list.GetRankOfName;local scores=list:GetHighScores()
        scores[1].GetName=function()collectgarbage('collect');return 'saved'end
        PROFILEMAN={};manager=nil;player=nil;list=nil;scores=nil;collectgarbage('collect')
        set({},'after collection');return query(),rank({},'saved')
    "#);
    // The Rust host explicitly traces only the captured object graph. Native mlua
    // handles do not provide the same collection contract for closure/table cycles.
    let mut runtime = SongRuntime::new(&SongLuaCompileContext::new(
        Path::new("."),
        "profile lifetime",
    ))
    .unwrap();
    execute(
        &mut runtime,
        r#"
        local m=PROFILEMAN;local p=m:GetProfile(0);local other=m:GetProfile(1)
        local weak=setmetatable({m,p,other},{__mode='v'});keep=p.GetDisplayName
        PROFILEMAN=nil;m=nil;p=nil;other=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak[1]==nil and weak[2]~=nil and weak[3]==nil and keep()=='Player 1')
        keep=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[2]==nil)
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
