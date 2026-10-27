//! Resolve browser model labels at display time, keeping metadata untouched.
use std::sync::Arc;

pub(super) fn text(source: &str) -> Arc<str> {
    let key = match source {
        "NEW SONGS INSTALLED" => "MessageNEWSONGSINSTALLED",
        "NEW CONTENT INSTALLED" => "MessageNEWCONTENTINSTALLED",
        "Reloading songs" => "MessageReloadingSongs",
        "Library changed." => "MessageLibraryChanged",
        "Reload songs" => "MessageReloadSongs",
        "Not yet" => "MessageNotYet",
        "SYNC THIS PACK" => "MessageSYNCTHISPACK",
        "1 song" => "Message1Song",
        "Moving offsets" => "MessageMovingOffsets",
        "in its Pack.ini, so the same shift is never made twice." => {
            "MessageInItsPackIniSoTheSameShiftIs"
        }
        "START shift    BACK go back" => "MessageSTARTShiftBACKGoBack",
        "its Pack.ini says ITG" => "MessageItsPackIniSaysITG",
        "its Pack.ini says NULL" => "MessageItsPackIniSaysNULL",
        "it has no sync in a Pack.ini" => "MessageItHasNoSyncInAPackIni",
        "MEASURE WITH NULL-OR-DIE" => "MessageMEASUREWITHNULLORDIE",
        "measure every song against its music, review the offsets, then save" => {
            "MessageMeasureEverySongAgainstItsMusicReviewTheOffsets"
        }
        "leave the simfiles alone; the engine applies it while Pack.ini Offsets is on" => {
            "MessageLeaveTheSimfilesAloneTheEngineAppliesItWhile"
        }
        "Checking the pack's folder..." => "MessageCheckingThePackSFolder",
        "Each saved simfile keeps a .old copy, and the pack is then recorded as NULL." => {
            "MessageEachSavedSimfileKeepsAOldCopyAndThe"
        }
        "Only the Pack.ini changes." => "MessageOnlyThePackIniChanges",
        "UP/DOWN choose    LEFT/RIGHT ITG or NULL    START save    BACK close" => {
            "MessageUPDOWNChooseLEFTRIGHTITGOrNULLSTART"
        }
        "UP/DOWN choose    START go    BACK close" => "MessageUPDOWNChooseSTARTGoBACKClose",
        "Search packs, chart authors and songs" => "MessageSearchPacksChartAuthorsAndSongs",
        "&MENUDOWN; more" => "MessageMENUDOWNMore",
        "loading the catalogue" => "MessageLoadingTheCatalogue",
        "could not reach stepmaniaonline.net" => "MessageCouldNotReachStepmaniaonlineNet",
        "searching names, charters and songs" => "MessageSearchingNamesChartersAndSongs",
        "could not load the popular packs to look through" => {
            "MessageCouldNotLoadThePopularPacksToLookThrough"
        }
        "none found in the packs checked so far" => "MessageNoneFoundInThePacksCheckedSoFar",
        "nothing matches that search" => "MessageNothingMatchesThatSearch",
        " songs  -  " => "MessageSongscc1b2d",
        "  -  added " => "MessageAdded",
        "In Library" => "MessageInLibrary",
        "MADE FOR DOUBLES" => "MessageMADEFORDOUBLES",
        "SOME DOUBLES" => "MessageSOMEDOUBLES",
        "Nothing here." => "MessageNothingHere",
        "sync: ITG" => "MessageSyncITG",
        "sync: NULL" => "MessageSyncNULL",
        "sync: not set" => "MessageSyncNotSet",
        "could not load details" => "MessageCouldNotLoadDetails",
        "BACK exit" => "MessageBACKExit",
        "BACK done" => "MessageBACKDone",
        "BACK back" => "MessageBACKBack",
        "LEFT/RIGHT choose   START go" => "MessageLEFTRIGHTChooseSTARTGo",
        "UP/DOWN difficulty   SELECT/START stop the preview" => {
            "MessageUPDOWNDifficultySELECTSTARTStopThePreview"
        }
        "UP/DOWN songs   SELECT/START stop the preview" => {
            "MessageUPDOWNSongsSELECTSTARTStopThePreview"
        }
        "START download this pack   DOWN back to the songs" => {
            "MessageSTARTDownloadThisPackDOWNBackToTheSongs"
        }
        "SELECT try again   START download" => "MessageSELECTTryAgainSTARTDownload",
        "START download" => "MessageSTARTDownload",
        "UP/DOWN songs   LEFT/RIGHT page   SELECT preview   START song options" => {
            "MessageUPDOWNSongsLEFTRIGHTPageSELECTPreviewSTART"
        }
        "LEFT/RIGHT views   START type a search" => "MessageLEFTRIGHTViewsSTARTTypeASearch",
        "LEFT/RIGHT views   DOWN options" => "MessageLEFTRIGHTViewsDOWNOptions",
        "LEFT/RIGHT views   DOWN open   SELECT reload" => {
            "MessageLEFTRIGHTViewsDOWNOpenSELECTReload"
        }
        "LEFT/RIGHT year   DOWN packs   UP views   SELECT reload" => {
            "MessageLEFTRIGHTYearDOWNPacksUPViewsSELECTReload"
        }
        "LEFT/RIGHT packs   START details   DOWN the list" => {
            "MessageLEFTRIGHTPacksSTARTDetailsDOWNTheList"
        }
        "LEFT/RIGHT list   DOWN open   UP views" => "MessageLEFTRIGHTListDOWNOpenUPViews",
        "UP/DOWN browse   LEFT/RIGHT page   START details" => {
            "MessageUPDOWNBrowseLEFTRIGHTPageSTARTDetails"
        }
        "UP/DOWN  LEFT/RIGHT browse   START sync   SELECT delete" => {
            "MessageUPDOWNLEFTRIGHTBrowseSTARTSyncSELECTDelete"
        }
        "UP/DOWN browse   LEFT/RIGHT page   START details   SELECT reload" => {
            "MessageUPDOWNBrowseLEFTRIGHTPageSTARTDetailsSELECT"
        }
        "NULL (0 ms), per SMO" => "MessageNULL0MsPerSMO",
        "mixed, per SMO" => "MessageMixedPerSMO",
        "not listed by SMO" => "MessageNotListedBySMO",
        "SEARCH" => "MessageSEARCH",
        "PAD" => "MessagePAD",
        "KEYBOARD" => "MessageKEYBOARD",
        "BEGINNER" => "MessageBEGINNER",
        "DOUBLES" => "MessageDOUBLES",
        "STAMINA" => "MessageSTAMINA",
        "ALL AROUND" => "MessageALLAROUND",
        "YEARS" => "MessageYEARS",
        "RIP PACKS" => "MessageRIPPACKS",
        "INSTALLED" => "MessageINSTALLED",
        "nothing from this catalogue is in your library yet" => {
            "MessageNothingFromThisCatalogueIsInYourLibraryYet"
        }
        "no packs were added in that year" => "MessageNoPacksWereAddedInThatYear",
        "no doubles packs found" => "MessageNoDoublesPacksFound",
        "no beginner-friendly packs found" => "MessageNoBeginnerFriendlyPacksFound",
        "no packs in this view" => "MessageNoPacksInThisView",
        "nothing was deleted" => "MessageNothingWasDeleted",
        "deleted 1 song folder" => "MessageDeleted1SongFolder",
        "SEARCH RESULTS" => "MessageSEARCHRESULTS",
        "in pack names, chart authors and song titles." => {
            "MessageInPackNamesChartAuthorsAndSongTitles"
        }
        "KEYBOARD PACKS" => "MessageKEYBOARDPACKS",
        "Packs stepmaniaonline.net lists as keyboard." => {
            "MessagePacksStepmaniaonlineNetListsAsKeyboard"
        }
        "Packs stepmaniaonline.net lists as technical or all-around." => {
            "MessagePacksStepmaniaonlineNetListsAsTechnicalOrAllAround"
        }
        "HARD CONTENT / STAMINA" => "MessageHARDCONTENTSTAMINA",
        "Packs stepmaniaonline.net lists as stamina." => {
            "MessagePacksStepmaniaonlineNetListsAsStamina"
        }
        "Packs made for doubles, then packs with doubles charts in them." => {
            "MessagePacksMadeForDoublesThenPacksWithDoublesCharts"
        }
        "BEGINNER FRIENDLY" => "MessageBEGINNERFRIENDLY",
        "Popular packs whose beginner charts are mostly 1s to 4s." => {
            "MessagePopularPacksWhoseBeginnerChartsAreMostly1sTo"
        }
        "POPULAR PACKS" => "MessagePOPULARPACKS",
        "JUST ADDED" => "MessageJUSTADDED",
        "OLDER" => "MessageOLDER",
        " and more" => "MessageAndMore",
        "Charts by" => "MessageChartsBy",
        "unknown error" => "MessageUnknownError",
        "already in your library" => "MessageAlreadyInYourLibrary",
        "This pack lists no songs" => "MessageThisPackListsNoSongs",
        "Could not load this pack's song list" => "MessageCouldNotLoadThisPackSSongList",
        "INSTALLED - RELOAD SONGS" => "MessageINSTALLEDRELOADSONGS",
        "DOWNLOAD FAILED - RETRY" => "MessageDOWNLOADFAILEDRETRY",
        "IN YOUR LIBRARY" => "MessageINYOURLIBRARY",
        "UP/DOWN move   START choose   SELECT clear" => "MessageUPDOWNMoveSTARTChooseSELECTClear",
        "UP/DOWN move   LEFT/RIGHT page   START choose   SELECT clear" => {
            "MessageUPDOWNMoveLEFTRIGHTPageSTARTChooseSELECT"
        }
        "LEFT/RIGHT keep or re-rip   START choose   UP options" => {
            "MessageLEFTRIGHTKeepOrReRipSTARTChooseUP"
        }
        "START reset options   UP options" => "MessageSTARTResetOptionsUPOptions",
        "UP options" => "MessageUPOptions",
        "START rip a pack   UP options" => "MessageSTARTRipAPackUPOptions",
        "START type names   SELECT clear   UP/DOWN options" => {
            "MessageSTARTTypeNamesSELECTClearUPDOWNOptions"
        }
        "START choose eras   SELECT any era   UP/DOWN options" => {
            "MessageSTARTChooseErasSELECTAnyEraUPDOWNOptions"
        }
        "START choose authors   SELECT anyone   UP/DOWN options" => {
            "MessageSTARTChooseAuthorsSELECTAnyoneUPDOWNOptions"
        }
        "LEFT/RIGHT change   START type it   UP/DOWN options" => {
            "MessageLEFTRIGHTChangeSTARTTypeItUPDOWNOptions"
        }
        "LEFT/RIGHT change   UP/DOWN options" => "MessageLEFTRIGHTChangeUPDOWNOptions",
        "15 SONGS" => "Message15SONGS",
        "- - - TEAR HERE - - -" => "MessageTEARHERE",
        "CHOOSE ERAS" => "MessageCHOOSEERAS",
        "CHOOSE AUTHORS" => "MessageCHOOSEAUTHORS",
        "any era" => "MessageAnyEra",
        "OR TYPE AUTHORS" => "MessageORTYPEAUTHORS",
        "AVOID CHARTS BY" => "MessageAVOIDCHARTSBY",
        "EXPERT FROM" => "MessageEXPERTFROM",
        "EXPERT TO" => "MessageEXPERTTO",
        "FULL DIFFICULTIES" => "MessageFULLDIFFICULTIES",
        "Chart authors, separated by commas" => "MessageChartAuthorsSeparatedByCommas",
        "Chart authors to avoid, separated by commas" => {
            "MessageChartAuthorsToAvoidSeparatedByCommas"
        }
        "Lowest Expert meter (1 to 30)" => "MessageLowestExpertMeter1To30",
        "Highest Expert meter (1 to 30)" => "MessageHighestExpertMeter1To30",
        "Any era  (START to choose)" => "MessageAnyEraSTARTToChoose",
        "loading the list..." => "MessageLoadingTheList",
        "list unavailable  (START to retry)" => "MessageListUnavailableSTARTToRetry",
        "Anyone  (START to choose)" => "MessageAnyoneSTARTToChoose",
        "none  (START to type exact names)" => "MessageNoneSTARTToTypeExactNames",
        "nobody  (START to type exact names)" => "MessageNobodySTARTToTypeExactNames",
        "Yes: Beginner to Challenge" => "MessageYesBeginnerToChallenge",
        "ANY ERA" => "MessageANYERA",
        "  -  EXPERT " => "MessageEXPERT",
        "  -  FULL" => "MessageFULL",
        "LOOKING FOR SONGS..." => "MessageLOOKINGFORSONGS",
        "KEEP THIS PACK?" => "MessageKEEPTHISPACK",
        "RIP A BOOSTER PACK" => "MessageRIPABOOSTERPACK",
        "NO PACK THIS TIME" => "MessageNOPACKTHISTIME",
        "none of the songs could be downloaded" => "MessageNoneOfTheSongsCouldBeDownloaded",
        "the rip did not work" => "MessageTheRipDidNotWork",
        "15 random songs within the limits on the left, made into a pack of their own" => {
            "Message15RandomSongsWithinTheLimitsOnTheLeft"
        }
        "RIP A PACK" => "MessageRIPAPACK",
        "15 random songs, each from a different pack on stepmaniaonline.net, as a new pack in your library." => {
            "Message15RandomSongsEachFromADifferentPackOn"
        }
        "a pack is still downloading" => "MessageAPackIsStillDownloading",
        "a pack is still being ripped" => "MessageAPackIsStillBeingRipped",
        "no sample for this song" => "MessageNoSampleForThisSong",
        "playing a sample" => "MessagePlayingASample",
        "loading sample..." => "MessageLoadingSamplee9255e",
        "that song is already on its way" => "MessageThatSongIsAlreadyOnItsWay",
        "that song was already added this session" => "MessageThatSongWasAlreadyAddedThisSession",
        "later" => "MessageLater",
        "earlier" => "MessageEarlier",
        "song" => "MessageSong",
        "songs" => "MessageSongs9780da",
        "content" => "MessageContent",
        "pack" => "MessagePack",
        "packs" => "MessagePacksdbd1af",
        "Starting..." => "MessageStarting",
        "queued" => "MessageQueued",
        "Installing" => "MessageInstalling",
        "Installed" => "MessageInstalledf8b32f",
        "Error" => "MessageError",
        "January" => "MessageJanuary",
        "February" => "MessageFebruary",
        "March" => "MessageMarch",
        "April" => "MessageApril",
        "May" => "MessageMay",
        "June" => "MessageJune",
        "July" => "MessageJuly",
        "August" => "MessageAugust",
        "September" => "MessageSeptember",
        "October" => "MessageOctober",
        "November" => "MessageNovember",
        "December" => "MessageDecember",
        "KEEP" => "MessageKEEP",
        "RE-RIP" => "MessageRERIP",
        "No" => "MessageNo",
        "Yes" => "MessageYes",
        "ANY" => "MessageANY",
        "NONE" => "MessageNONE",
        "FULL" => "MessageFULLcb6839",
        "EXPERT" => "MessageEXPERT884981",
        "ERAS" => "MessageERAS",
        "AUTHORS" => "MessageAUTHORSab6af7",
        "itgdb.net could not be reached, so this list is unavailable." => {
            "MessageItgdbNetCouldNotBeReachedSoThisList"
        }
        "could not be downloaded" => "MessageCouldNotBeDownloaded",
        "taken as ITG-synced: its offset is moved 9 ms to NULL as it installs" => {
            "MessageTakenAsITGSyncedItsOffsetIsMoved9"
        }
        "already in your library - remove it from the Installed tab if you want it again" => {
            "MessageAlreadyInYourLibraryRemoveItFromTheInstalled"
        }
        "(this pack is downloading already)" => "MessageThisPackIsDownloadingAlready",
        "Preview" => "MessagePreview",
        "Get this song" => "MessageGetThisSong",
        "LEFT/RIGHT choose    UP/DOWN difficulty    START go    BACK cancel" => {
            "MessageLEFTRIGHTChooseUPDOWNDifficultySTARTGoBACK"
        }
        "LEFT/RIGHT choose    START go    BACK cancel" => "MessageLEFTRIGHTChooseSTARTGoBACKCancel",
        "Songs" => "MessageSongsbc6761",
        "Charts" => "MessageCharts",
        "Size" => "MessageSize",
        "Difficulty" => "MessageDifficulty",
        "Style" => "MessageStyle",
        "Sync" => "MessageSync8d261a",
        "Added" => "MessageAdded6b02e0",
        "Keyboard" => "MessageKeyboard",
        "Doubles" => "MessageDoubles",
        "Singles" => "MessageSingles",
        "Mixed" => "MessageMixed",
        "ERA" => "MessageERA",
        "AUTHOR" => "MessageAUTHOR",
        "PAD+KEY" => "MessagePADKEY",
        "KEY" => "MessageKEY",
        "RIPPING..." => "MessageRIPPING",
        "INSTALLING..." => "MessageINSTALLING",
        "QUEUED" => "MessageQUEUED",
        "installing..." => "MessageInstallingca20fb",
        "installed" => "MessageInstalleda51a6c",
        "SINGLES" => "MessageSINGLES",
        "BOOSTER" => "MessageBOOSTER",
        "CONTENT" => "MessageCONTENT",
        "HARDEST" => "MessageHARDEST",
        "anyone" => "MessageAnyone",
        "Jan" => "MessageJan",
        "Feb" => "MessageFeb",
        "Mar" => "MessageMar",
        "Apr" => "MessageApr",
        "Jun" => "MessageJun",
        "Jul" => "MessageJul",
        "Aug" => "MessageAug",
        "Sep" => "MessageSep",
        "Oct" => "MessageOct",
        "Nov" => "MessageNov",
        "Dec" => "MessageDec",
        _ => return Arc::from(source),
    };
    crate::i18n::tr("ContentBrowser", key)
}
