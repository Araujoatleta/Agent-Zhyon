; Coucou's NSIS hooks (bundle > windows > nsis > installerHooks): the "Your assistant" page of the setup.exe, and
; the uninstaller's clean-up.
;
; installer.nsi (Coucou's copy of Tauri's template, next to this file) includes this file at the top level, right
; after MUI2, FileFunc, StrFunc and utils.nsh and before its own !defines (PRODUCTNAME, MAINBINARYNAME...), its Vars
; ($PassiveMode, $UpdateMode...) and its !addplugindir. So the Functions here use MUI2, LogicLib, nsDialogs, FileFunc,
; System and their own Vars only: no ${PRODUCTNAME}, no nsis_tauri_utils::. The NSIS_HOOK_* macros are expanded
; later, inside the template's sections. installer.nsi declares the page right after the install-folder page.
;
; Nothing secret is written to disk: a key typed on the page goes straight into Windows Credential Manager, under the
; very entry Coucou reads (secrets.rs); the other choices go to %APPDATA%\Coucou\setup.json, which Coucou reads and
; deletes at its next start (first_run.rs) before setting Hermes up the way Settings → Hermes Agent does.

; ── What the page offers ─────────────────────────────────────────────────────

; The brains, in the dropdown's order: index, Coucou's id (hermes_setup.rs BRAINS), its keystore entry ("" when it
; needs no key), the address of the server on this computer it uses ("" when none, only shown), and its label in
; English, French and Portuguese (Brazil).
!macro CoucouBrains EACH
  !insertmacro ${EACH} 0 "chatgpt"    ""                   ""                        "ChatGPT (your subscription)"  "ChatGPT (votre abonnement)"     "ChatGPT (sua assinatura)"
  !insertmacro ${EACH} 1 "openai"     "openai-api-key"     ""                        "OpenAI (API key)"             "OpenAI (clé d'API)"             "OpenAI (chave de API)"
  !insertmacro ${EACH} 2 "anthropic"  "anthropic-api-key"  ""                        "Anthropic Claude (API key)"   "Anthropic Claude (clé d'API)"   "Anthropic Claude (chave de API)"
  !insertmacro ${EACH} 3 "google"     "google-api-key"     ""                        "Google Gemini (API key)"      "Google Gemini (clé d'API)"      "Google Gemini (chave de API)"
  !insertmacro ${EACH} 4 "openrouter" "openrouter-api-key" ""                        "OpenRouter (API key)"         "OpenRouter (clé d'API)"         "OpenRouter (chave de API)"
  !insertmacro ${EACH} 5 "ollama"     ""                   "http://127.0.0.1:11434"  "Ollama (on this computer)"    "Ollama (sur cet ordinateur)"    "Ollama (neste computador)"
  !insertmacro ${EACH} 6 "lmstudio"   ""                   "http://127.0.0.1:1234"   "LM Studio (on this computer)" "LM Studio (sur cet ordinateur)" "LM Studio (neste computador)"
!macroend

; The tones (persona ids), the default first. "custom" is for Settings: it is the user's own text.
!macro CoucouPersonas EACH
  !insertmacro ${EACH} 0 "friendly"     "Friendly"               "Amical"                             "Amigável"
  !insertmacro ${EACH} 1 "direct"       "Direct"                 "Direct"                             "Direto"
  !insertmacro ${EACH} 2 "professional" "Professional"           "Professionnel"                      "Profissional"
  !insertmacro ${EACH} 3 "playful"      "Playful"                "Enjoué"                             "Divertido"
  !insertmacro ${EACH} 4 "butler"       "Butler (formal, witty)" "Majordome (formel, plein d'esprit)" "Mordomo (formal e espirituoso)"
!macroend

; The skill packs, in the page's reading order: index, id, ticked by default (1) or not (0), and the checkbox's label.
!macro CoucouSkills EACH
  !insertmacro ${EACH} 0 "home"   1 "Mochi's home (TVs, lights…)"  "La maison de Mochi (TV, lumières…)"  "Casa do Mochi (TVs, luzes…)"
  !insertmacro ${EACH} 1 "news"   0 "News and feeds (RSS)"         "Actualités et flux (RSS)"            "Notícias e feeds (RSS)"
  !insertmacro ${EACH} 2 "search" 1 "Free web search (DuckDuckGo)" "Recherche web gratuite (DuckDuckGo)" "Busca na web grátis (DuckDuckGo)"
!macroend

; Where each skill's checkbox goes (x y width): the first two side by side, the third, the longest label, below.
!define COUCOU_SKILL_AT_0 "50u 73u 150u"
!define COUCOU_SKILL_AT_1 "202u 73u 98u"
!define COUCOU_SKILL_AT_2 "50u 84u 250u"

; ── The page ─────────────────────────────────────────────────────────────────

Var CoucouShown         ; 1 once the page has its values (the defaults, then what the user left)
Var CoucouEnable        ; 1 = set the assistant up at Coucou's first start
Var CoucouBrainIndex
Var CoucouBrain         ; a brain id of the table above
Var CoucouKeyEntry      ; its keystore entry, "" when it needs no key
Var CoucouAddress       ; the server on this computer it uses, "" when none
Var CoucouKey           ; secret: memory only, overwritten once stored
Var CoucouName
Var CoucouToneIndex
Var CoucouPersona
Var CoucouPhone
Var CoucouVpsUrl
Var CoucouDeviceName

Var CoucouHwEnable
Var CoucouHwBrainLabel
Var CoucouHwBrain
Var CoucouHwKeyLabel
Var CoucouHwKey
Var CoucouHwHint
Var CoucouHwNameLabel
Var CoucouHwName
Var CoucouHwToneLabel
Var CoucouHwTone
Var CoucouHwSkillsLabel
Var CoucouHwPhone
Var CoucouHwPlan

!macro CoucouSkillVars INDEX ID ON EN FR PT
  Var CoucouSkill${INDEX}               ; 1 = ticked
  Var CoucouHwSkill${INDEX}
!macroend
!insertmacro CoucouSkills CoucouSkillVars

; Coucou's installer.nsi declares the page itself, after the install folder. With Tauri's own template the page is
; declared here and comes first, before Welcome.
!ifndef COUCOU_PAGE_IN_TEMPLATE
  Page custom CoucouPageCreate CoucouPageLeave
!endif

; The text for the installer's language: English, French (1036) or Portuguese, Brazil (1046); English for any other.
; LangString can't be used here: ${LANG_FRENCH} and the others are only defined later, by MUI_LANGUAGE.
!macro CoucouText VAR EN FR PT
  ${If} $LANGUAGE = 1036
    StrCpy ${VAR} "${FR}"
  ${ElseIf} $LANGUAGE = 1046
    StrCpy ${VAR} "${PT}"
  ${Else}
    StrCpy ${VAR} "${EN}"
  ${EndIf}
!macroend

!macro CoucouBrainAdd INDEX ID KEY ADDRESS EN FR PT
  !insertmacro CoucouText $0 "${EN}" "${FR}" "${PT}"
  ${NSD_CB_AddString} $CoucouHwBrain $0
!macroend

!macro CoucouBrainPick INDEX ID KEY ADDRESS EN FR PT
  ${If} $CoucouBrainIndex = ${INDEX}
    StrCpy $CoucouBrain "${ID}"
    StrCpy $CoucouKeyEntry "${KEY}"
    StrCpy $CoucouAddress "${ADDRESS}"
  ${EndIf}
!macroend

!macro CoucouPersonaAdd INDEX ID EN FR PT
  !insertmacro CoucouText $0 "${EN}" "${FR}" "${PT}"
  ${NSD_CB_AddString} $CoucouHwTone $0
!macroend

!macro CoucouPersonaPick INDEX ID EN FR PT
  ${If} $CoucouToneIndex = ${INDEX}
    StrCpy $CoucouPersona "${ID}"
  ${EndIf}
!macroend

!macro CoucouSkillDefault INDEX ID ON EN FR PT
  StrCpy $CoucouSkill${INDEX} ${ON}
!macroend

!macro CoucouSkillAdd INDEX ID ON EN FR PT
  !insertmacro CoucouText $0 "${EN}" "${FR}" "${PT}"
  ${NSD_CreateCheckBox} ${COUCOU_SKILL_AT_${INDEX}} 10u $0
  Pop $CoucouHwSkill${INDEX}
  ${If} $CoucouSkill${INDEX} = 1
    ${NSD_Check} $CoucouHwSkill${INDEX}
  ${EndIf}
!macroend

!macro CoucouSkillEnable INDEX ID ON EN FR PT
  EnableWindow $CoucouHwSkill${INDEX} $1
!macroend

!macro CoucouSkillSave INDEX ID ON EN FR PT
  ${NSD_GetState} $CoucouHwSkill${INDEX} $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CoucouSkill${INDEX} 1
  ${Else}
    StrCpy $CoucouSkill${INDEX} 0
  ${EndIf}
!macroend

!macro CoucouSkillList INDEX ID ON EN FR PT
  ${If} $CoucouSkill${INDEX} = 1
    StrCpy $2 "$2,${ID}"
  ${EndIf}
!macroend

Function CoucouPageCreate
  ; Like the template's own pages, passive (/P) and update (/UPDATE) runs skip it; silent (/S) runs show no page.
  ${GetOptions} $CMDLINE "/P" $0
  ${IfNot} ${Errors}
    Abort
  ${EndIf}
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    Abort
  ${EndIf}

  ${If} $CoucouShown != 1
    ; A reinstall or an update finds Coucou's settings: the assistant it has stays as it is unless the box is ticked.
    ${If} ${FileExists} "$APPDATA\Coucou\settings.json"
      StrCpy $CoucouEnable 0
    ${Else}
      StrCpy $CoucouEnable 1
    ${EndIf}
    StrCpy $CoucouBrainIndex 0
    StrCpy $CoucouName "Zheus"
    StrCpy $CoucouToneIndex 0
    StrCpy $CoucouPhone 1
    !insertmacro CoucouSkills CoucouSkillDefault
    StrCpy $CoucouShown 1
  ${EndIf}

  !insertmacro CoucouText $0 "Your assistant" "Votre assistant" "Seu assistente"
  !insertmacro CoucouText $1 "Hermes Agent, free and open source by Nous Research, is the assistant that answers you in Mochi's chat, here and on your phone." "Hermes Agent, libre et gratuit, par Nous Research, est l'assistant qui vous répond dans le chat de Mochi, ici et sur votre téléphone." "O Hermes Agent, gratuito e de código aberto, da Nous Research, é o assistente que responde no chat do Mochi, aqui e no celular."
  !insertmacro MUI_HEADER_TEXT $0 $1

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

  !insertmacro CoucouText $0 "Set up the assistant now (recommended)" "Configurer l'assistant maintenant (recommandé)" "Configurar o assistente agora (recomendado)"
  ${NSD_CreateCheckBox} 0 0 300u 10u $0
  Pop $CoucouHwEnable
  ${If} $CoucouEnable = 1
    ${NSD_Check} $CoucouHwEnable
  ${EndIf}
  ${NSD_OnClick} $CoucouHwEnable CoucouPageUpdate

  !insertmacro CoucouText $0 "Brain" "Cerveau" "Cérebro"
  ${NSD_CreateLabel} 0 15u 48u 10u $0
  Pop $CoucouHwBrainLabel
  ${NSD_CreateDropList} 50u 13u 250u 90u ""
  Pop $CoucouHwBrain
  !insertmacro CoucouBrains CoucouBrainAdd
  ${NSD_CB_SetSelectionIndex} $CoucouHwBrain $CoucouBrainIndex
  ${NSD_OnChange} $CoucouHwBrain CoucouPageUpdate

  !insertmacro CoucouText $0 "API key" "Clé d'API" "Chave de API"
  ${NSD_CreateLabel} 0 29u 48u 10u $0
  Pop $CoucouHwKeyLabel
  ${NSD_CreatePassword} 50u 27u 250u 12u $CoucouKey
  Pop $CoucouHwKey
  ${NSD_Edit_SetTextLimit} $CoucouHwKey 512

  ; Says where the key goes, or how the brain gets ready without one (CoucouPageUpdate writes it).
  ${NSD_CreateLabel} 50u 41u 250u 16u ""
  Pop $CoucouHwHint

  !insertmacro CoucouText $0 "Name" "Nom" "Nome"
  ${NSD_CreateLabel} 0 61u 48u 10u $0
  Pop $CoucouHwNameLabel
  ${NSD_CreateText} 50u 59u 84u 12u $CoucouName
  Pop $CoucouHwName
  ${NSD_Edit_SetTextLimit} $CoucouHwName 40

  !insertmacro CoucouText $0 "Tone" "Ton" "Tom"
  ${NSD_CreateLabel} 140u 61u 30u 10u $0
  Pop $CoucouHwToneLabel
  ${NSD_CreateDropList} 172u 59u 128u 70u ""
  Pop $CoucouHwTone
  !insertmacro CoucouPersonas CoucouPersonaAdd
  ${NSD_CB_SetSelectionIndex} $CoucouHwTone $CoucouToneIndex

  !insertmacro CoucouText $0 "Skills" "Compétences" "Skills"
  ${NSD_CreateLabel} 0 74u 48u 10u $0
  Pop $CoucouHwSkillsLabel
  !insertmacro CoucouSkills CoucouSkillAdd

  !insertmacro CoucouText $0 "Let my iPhone change the assistant and use Mochi's home" "Laisser mon iPhone modifier l'assistant et utiliser la maison de Mochi" "Deixar meu iPhone alterar o assistente e usar a Casa do Mochi"
  ${NSD_CreateCheckBox} 0 95u 300u 10u $0
  Pop $CoucouHwPhone
  ${If} $CoucouPhone = 1
    ${NSD_Check} $CoucouHwPhone
  ${EndIf}

  ; What Coucou will do, said before the click.
  !insertmacro CoucouText $0 "When Coucou first starts it will: download Hermes Agent's official installer (a fixed version, checked by SHA-256), install Hermes and its Python in your user folder, set the brain, write the personality to Hermes' SOUL.md (an existing one is backed up), and install the ticked skills." "Au premier lancement, Coucou va : télécharger l'installeur officiel de Hermes Agent (une version fixe, vérifiée par SHA-256), installer Hermes et son Python dans votre dossier utilisateur, régler le cerveau, écrire la personnalité dans le SOUL.md de Hermes (l'ancien est sauvegardé) et installer les compétences cochées." "Na primeira vez que o Coucou abrir, ele vai: baixar o instalador oficial do Hermes Agent (uma versão fixa, conferida por SHA-256), instalar o Hermes e o Python dele na sua pasta de usuário, definir o cérebro, escrever a personalidade no SOUL.md do Hermes (o anterior ganha um backup) e instalar as skills marcadas."
  ${NSD_CreateLabel} 0 108u 300u 32u $0
  Pop $CoucouHwPlan

  ${NSD_OnBack} CoucouPageSave
  Push $CoucouHwEnable                                ; what NSD_OnClick would push
  Call CoucouPageUpdate
  nsDialogs::Show
FunctionEnd

; Unticked "Set up the assistant now": everything else is disabled. Ticked: the key field only for a brain with a
; key, and the hint for the brain picked.
Function CoucouPageUpdate
  Pop $0                                              ; the control that changed
  ${NSD_CB_GetSelectionIndex} $CoucouHwBrain $CoucouBrainIndex
  !insertmacro CoucouBrains CoucouBrainPick

  ${NSD_GetState} $CoucouHwEnable $1
  ${If} $1 == ${BST_CHECKED}
    StrCpy $1 1
  ${Else}
    StrCpy $1 0
  ${EndIf}
  EnableWindow $CoucouHwBrainLabel $1
  EnableWindow $CoucouHwBrain $1
  EnableWindow $CoucouHwHint $1
  EnableWindow $CoucouHwNameLabel $1
  EnableWindow $CoucouHwName $1
  EnableWindow $CoucouHwToneLabel $1
  EnableWindow $CoucouHwTone $1
  EnableWindow $CoucouHwSkillsLabel $1
  !insertmacro CoucouSkills CoucouSkillEnable
  EnableWindow $CoucouHwPhone $1
  EnableWindow $CoucouHwPlan $1

  StrCpy $2 0
  ${If} $1 = 1
  ${AndIf} $CoucouKeyEntry != ""
    StrCpy $2 1
  ${EndIf}
  EnableWindow $CoucouHwKeyLabel $2
  EnableWindow $CoucouHwKey $2

  ${If} $CoucouBrain == "chatgpt"
    !insertmacro CoucouText $3 "After installing, Coucou opens OpenAI's page and shows a code: sign in with your ChatGPT Plus or Pro account. No key needed." "Après l'installation, Coucou ouvre la page d'OpenAI et affiche un code : connectez-vous avec votre compte ChatGPT Plus ou Pro. Sans clé." "Depois de instalar, o Coucou abre a página da OpenAI e mostra um código: entre com sua conta ChatGPT Plus ou Pro. Sem chave."
  ${ElseIf} $CoucouKeyEntry != ""
    !insertmacro CoucouText $3 "Stored in Windows Credential Manager, never in a file." "Gardée dans le Gestionnaire d'identification Windows, jamais dans un fichier." "Guardada no Gerenciador de Credenciais do Windows, nunca em um arquivo."
  ${Else}
    !insertmacro CoucouText $3 "No key needed: Coucou uses the server on this computer ($CoucouAddress unless Settings already has another address)." "Pas de clé : Coucou utilise le serveur de cet ordinateur ($CoucouAddress, sauf autre adresse déjà dans les Réglages)." "Sem chave: o Coucou usa o servidor deste computador ($CoucouAddress, salvo outro endereço já nas Configurações)."
  ${EndIf}
  ${NSD_SetText} $CoucouHwHint $3
FunctionEnd

; Keeps what the page shows, for Next and for Back (the page is drawn again with it).
Function CoucouPageSave
  ${NSD_GetState} $CoucouHwEnable $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CoucouEnable 1
  ${Else}
    StrCpy $CoucouEnable 0
  ${EndIf}
  ${NSD_CB_GetSelectionIndex} $CoucouHwBrain $CoucouBrainIndex
  !insertmacro CoucouBrains CoucouBrainPick

  ; Trimmed like a key typed in Settings: spaces, tabs and line breaks at either end.
  ${NSD_GetText} $CoucouHwKey $CoucouKey
  ${Do}
    StrCpy $0 $CoucouKey 1
    ${If} $0 == " "
    ${OrIf} $0 == "$\t"
    ${OrIf} $0 == "$\r"
    ${OrIf} $0 == "$\n"
      StrCpy $CoucouKey $CoucouKey "" 1
    ${Else}
      ${ExitDo}
    ${EndIf}
  ${Loop}
  ${Do}
    StrCpy $0 $CoucouKey 1 -1
    ${If} $0 == " "
    ${OrIf} $0 == "$\t"
    ${OrIf} $0 == "$\r"
    ${OrIf} $0 == "$\n"
      StrCpy $CoucouKey $CoucouKey -1
    ${Else}
      ${ExitDo}
    ${EndIf}
  ${Loop}
  StrCpy $0 ""

  ${NSD_GetText} $CoucouHwName $CoucouName
  ${NSD_CB_GetSelectionIndex} $CoucouHwTone $CoucouToneIndex
  !insertmacro CoucouPersonas CoucouPersonaPick
  !insertmacro CoucouSkills CoucouSkillSave
  ${NSD_GetState} $CoucouHwPhone $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CoucouPhone 1
  ${Else}
    StrCpy $CoucouPhone 0
  ${EndIf}
FunctionEnd

Function CoucouPageLeave
  Call CoucouPageSave
  ; A brain that needs a key and got none: say so, stay on the page.
  ${If} $CoucouEnable = 1
  ${AndIf} $CoucouKeyEntry != ""
  ${AndIf} $CoucouKey == ""
    !insertmacro CoucouText $0 "This brain needs its API key: type it, or pick ChatGPT (your subscription) or a server on this computer." "Ce cerveau a besoin de sa clé d'API : saisissez-la, ou choisissez ChatGPT (votre abonnement) ou un serveur sur cet ordinateur." "Esse cérebro precisa da chave de API: digite a chave ou escolha o ChatGPT (sua assinatura) ou um servidor neste computador."
    MessageBox MB_ICONEXCLAMATION|MB_OK $0
    Abort
  ${EndIf}
FunctionEnd

; ── After the files are copied ───────────────────────────────────────────────

; Overwrites a copy of the key before emptying it: StrCpy "" alone only ends the string at its first character.
; 512 characters, the key field's limit.
!macro CoucouForget VAR
  StrCpy ${VAR} "################"
  StrCpy ${VAR} "${VAR}${VAR}${VAR}${VAR}${VAR}${VAR}${VAR}${VAR}"
  StrCpy ${VAR} "${VAR}${VAR}${VAR}${VAR}"
  StrCpy ${VAR} ""
!macroend

; Writes $CoucouKey into Windows Credential Manager exactly as keyring 3.6.3 (windows-native) does for
; Entry::new("fr.louisraille.coucou", <entry>), which is what secrets.rs reads: a generic credential persisted
; ENTERPRISE, TargetName "<entry>.fr.louisraille.coucou", UserName "<entry>", and as its blob the key's UTF-16LE code
; units without a terminator. <entry> is $CoucouKeyEntry. Leaves 1 in $0 when it is stored, else 0 and the Windows
; error in $1.
; The CREDENTIALW below is the 32-bit one (13 fields of 4 bytes, 52 bytes): Tauri builds the setup.exe from NSIS'
; x86-unicode stubs.
!if ${NSIS_PTR_SIZE} > 4
  !error "CoucouStoreKey lays CREDENTIALW out for a 32-bit installer"
!endif
Function CoucouStoreKey
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7
  Push $8
  Push $9
  StrCpy $0 0
  StrCpy $1 0
  StrCpy $9 $CoucouKey                                ; System reads registers only
  StrLen $2 $9
  IntOp $2 $2 * 2                                     ; CredentialBlobSize, in bytes
  StrCpy $3 "$CoucouKeyEntry.fr.louisraille.coucou"
  StrCpy $8 $CoucouKeyEntry
  System::Alloc $2                                    ; the blob
  Pop $4
  System::Call "kernel32::RtlMoveMemory(p r4, w r9, i r2)"
  System::Call "*(&w${NSIS_MAX_STRLEN} r3)p.r5"       ; TargetName
  System::Call "*(&w${NSIS_MAX_STRLEN} r8)p.r6"       ; UserName
  ; Flags, Type = GENERIC, TargetName, Comment, LastWritten (2 DWORDs), CredentialBlobSize, CredentialBlob,
  ; Persist = ENTERPRISE, AttributeCount, Attributes, TargetAlias, UserName
  System::Call "*(i 0, i 1, p r5, p 0, i 0, i 0, i r2, p r4, i 3, i 0, p 0, p 0, p r6)p.r7"
  System::Call "advapi32::CredWriteW(p r7, i 0)i.r0 ?e"
  Pop $1                                              ; GetLastError
  ${If} $0 <> 0
    StrCpy $0 1
    StrCpy $1 0
  ${EndIf}
  System::Call "kernel32::RtlZeroMemory(p r4, i r2)"
  !insertmacro CoucouForget $9
  System::Free $4
  System::Free $5
  System::Free $6
  System::Free $7
  Pop $9
  Pop $8
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
FunctionEnd

; Replaces the text on the top of the stack with its JSON string body: \ and " escaped, control characters (below
; U+0020) and halves of surrogate pairs found alone dropped, so that the file is valid UTF-16 and valid JSON. Each
; character is looked at by its UTF-16 code unit: StrCmp goes through the Windows collation, which takes some
; characters for others.
Function CoucouJson
  Exch $0                                             ; the text
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7
  StrCpy $1 ""                                        ; the result
  StrLen $2 $0
  System::Call "*(&w${NSIS_MAX_STRLEN} r0)p.r3"       ; a copy of the text, to read its code units
  StrCpy $4 0
  ${DoWhile} $4 < $2
    IntOp $5 $4 * 2
    IntPtrOp $5 $3 + $5
    System::Call "*$5(&i2 .r5, &i2 .r7)"              ; this code unit and the next one (0 after the last)
    StrCpy $6 $0 1 $4
    ${If} $5 = 0x5C
      StrCpy $1 "$1\\"
    ${ElseIf} $5 = 0x22
      StrCpy $1 '$1\"'
    ${ElseIf} $5 < 0x20
      ; a control character: dropped
    ${ElseIf} $5 >= 0xD800
    ${AndIf} $5 <= 0xDBFF
      ${If} $7 >= 0xDC00                              ; a surrogate pair: both halves kept
      ${AndIf} $7 <= 0xDFFF
        StrCpy $6 $0 2 $4
        StrCpy $1 "$1$6"
        IntOp $4 $4 + 1
      ${EndIf}
    ${ElseIf} $5 >= 0xDC00
    ${AndIf} $5 <= 0xDFFF
      ; the second half of a pair, alone: dropped
    ${Else}
      StrCpy $1 "$1$6"
    ${EndIf}
    IntOp $4 $4 + 1
  ${Loop}
  System::Free $3
  StrCpy $0 $1
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Exch $0
FunctionEnd

; The key first, straight into the OS keystore; then the choices, nothing secret, for Coucou's next start. Nothing
; at all when the page was skipped (/S, /P, /UPDATE): Coucou then offers the setup in Settings, as it always has.
Function CoucouSaveChoices
  ${If} $CoucouShown != 1
    Return
  ${EndIf}
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4

  ${If} $CoucouEnable = 1
  ${AndIf} $CoucouKeyEntry != ""
  ${AndIf} $CoucouKey != ""
    Call CoucouStoreKey
    ${If} $0 = 1
      !insertmacro CoucouText $4 "Coucou: the key is in Windows Credential Manager ($CoucouKeyEntry)." "Coucou : la clé est dans le Gestionnaire d'identification Windows ($CoucouKeyEntry)." "Coucou: a chave está no Gerenciador de Credenciais do Windows ($CoucouKeyEntry)."
    ${Else}
      !insertmacro CoucouText $4 "Coucou: the key could not be stored (error $1): add it in Coucou's Settings." "Coucou : la clé n'a pas pu être enregistrée (erreur $1) : ajoutez-la dans les Réglages de Coucou." "Coucou: não foi possível guardar a chave (erro $1): adicione nas Configurações do Coucou."
    ${EndIf}
    DetailPrint $4
  ${EndIf}
  !insertmacro CoucouForget $CoucouKey

  ; An unticked box still writes its file, so that choices an earlier install left unread are not applied.
  ${If} $CoucouEnable = 1
    Push $CoucouName
    Call CoucouJson
    Pop $1
    StrCpy $2 ""
    !insertmacro CoucouSkills CoucouSkillList
    StrCpy $2 $2 "" 1                                 ; without the first comma
    ${If} $CoucouPhone = 1
      StrCpy $3 "true"
    ${Else}
      StrCpy $3 "false"
    ${EndIf}
    StrCpy $1 '{"version":1,"configure":true,"brain":"$CoucouBrain","name":"$1","persona":"$CoucouPersona","skills":"$2","phone":$3}'
  ${Else}
    StrCpy $1 '{"version":1,"configure":false}'
  ${EndIf}

  ; UTF-16LE after a BOM: FileWrite would turn a name in another script into the ANSI code page.
  ; (${Errors} clears the error flag it reads, hence $2.)
  StrCpy $2 1
  CreateDirectory "$APPDATA\Coucou"
  ClearErrors
  FileOpen $0 "$APPDATA\Coucou\setup.json" w
  ${If} ${Errors}
    StrCpy $2 0
  ${Else}
    FileWriteUTF16LE /BOM $0 $1
    ${IfThen} ${Errors} ${|} StrCpy $2 0 ${|}
    FileClose $0
  ${EndIf}
  ${If} $2 = 0
    !insertmacro CoucouText $4 "Coucou: could not write $APPDATA\Coucou\setup.json: set the assistant up in Coucou's Settings." "Coucou : impossible d'écrire $APPDATA\Coucou\setup.json : configurez l'assistant dans les Réglages de Coucou." "Coucou: não foi possível gravar $APPDATA\Coucou\setup.json: configure o assistente nas Configurações do Coucou."
    DetailPrint $4
  ${ElseIf} $CoucouEnable = 1
    !insertmacro CoucouText $4 "Coucou: your choices are kept for its next start ($APPDATA\Coucou\setup.json)." "Coucou : vos choix sont gardés pour son prochain lancement ($APPDATA\Coucou\setup.json)." "Coucou: suas escolhas ficam guardadas para a próxima abertura ($APPDATA\Coucou\setup.json)."
    DetailPrint $4
  ${EndIf}

  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; ── Hooks ────────────────────────────────────────────────────────────────────

!macro NSIS_HOOK_PREINSTALL
  ; Release the executable before an update attempts to replace it.
  ExecWait '"$SYSDIR\schtasks.exe" /End /TN "Coucou Edge Worker"' $0
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Call CoucouSaveChoices
  ; These are non-secret settings. The durable identity stays in Credential Manager.
  StrCpy $CoucouVpsUrl "https://coucou-relay.navellux.com.br/control"
  StrCpy $CoucouDeviceName "$COMPUTERNAME"
  ${GetOptions} $CMDLINE "/VPSURL=" $0
  ${IfNot} ${Errors}
    StrCpy $CoucouVpsUrl $0
  ${EndIf}
  ${GetOptions} $CMDLINE "/DEVICENAME=" $0
  ${IfNot} ${Errors}
    StrCpy $CoucouDeviceName $0
  ${EndIf}
  StrCpy $0 $CoucouVpsUrl 8
  ${If} $0 != "https://"
    MessageBox MB_ICONSTOP|MB_OK "COUCOU_VPS_URL must use HTTPS."
  ${Else}
    WriteRegExpandStr HKCU "Environment" "COUCOU_VPS_URL" "$CoucouVpsUrl"
    WriteRegExpandStr HKCU "Environment" "COUCOU_DEVICE_NAME" "$CoucouDeviceName"
    System::Call 'kernel32::SetEnvironmentVariableW(w "COUCOU_VPS_URL", w "$CoucouVpsUrl")'
    System::Call 'kernel32::SetEnvironmentVariableW(w "COUCOU_DEVICE_NAME", w "$CoucouDeviceName")'
    ; Validate the exact runtime configuration before registering startup.
    ExecWait '"$INSTDIR\coucou-edge-worker.exe" --validate-config' $0
    ${If} $0 = 0
      WriteRegStr HKCU "Software\Coucou\EdgeWorker" "IdentityServer" "$CoucouVpsUrl"
      ; Stop only the instance owned by our task, then replace and run it in
      ; the interactive user's session (never as a service/SYSTEM).
      ExecWait '"$SYSDIR\schtasks.exe" /End /TN "Coucou Edge Worker"' $1
      ExecWait '"$SYSDIR\schtasks.exe" /Create /TN "Coucou Edge Worker" /SC ONLOGON /TR "$\"$INSTDIR\coucou-edge-worker.exe$\"" /RL LIMITED /F' $0
      ${If} $0 = 0
        ExecWait '"$SYSDIR\schtasks.exe" /Run /TN "Coucou Edge Worker"' $1
      ${EndIf}
    ${Else}
      MessageBox MB_ICONSTOP|MB_OK "Invalid edge worker configuration."
    ${EndIf}
  ${EndIf}
!macroend

; The app stages coucou-hook.exe into %LOCALAPPDATA%\Coucou\bin at launch, so the
; installer never recorded it and the default uninstaller leaves it behind. The
; inbox and the log live in the same place and are ours too.
;
; Claude Code's own settings.json is deliberately NOT touched here: it belongs to
; the user, it may contain hooks from other tools, and rewriting somebody's
; config from an uninstaller with no diff and no consent is exactly what the rest
; of this app goes out of its way not to do. A relay that is gone exits 0 without
; printing anything, so a leftover entry costs nothing beyond a dead path.

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$SYSDIR\schtasks.exe" /End /TN "Coucou Edge Worker"' $0
  ExecWait '"$SYSDIR\schtasks.exe" /Delete /TN "Coucou Edge Worker" /F' $0
  ReadRegStr $1 HKCU "Software\Coucou\EdgeWorker" "IdentityServer"
  ${If} $1 == ""
    StrCpy $1 "$CoucouVpsUrl"
  ${EndIf}
  ExecWait '"$INSTDIR\coucou-edge-worker.exe" --remove-identity --identity-server "$1"' $0
  DeleteRegKey HKCU "Software\Coucou\EdgeWorker"
  DeleteRegValue HKCU "Environment" "COUCOU_VPS_URL"
  DeleteRegValue HKCU "Environment" "COUCOU_DEVICE_NAME"
  RMDir /r "$LOCALAPPDATA\Coucou\edge-worker"
  RMDir /r "$LOCALAPPDATA\Coucou\bin"
  RMDir /r "$LOCALAPPDATA\Coucou\inbox"
  Delete "$LOCALAPPDATA\Coucou\coucou.log"
!macroend
