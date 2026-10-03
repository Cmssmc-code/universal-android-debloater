//! The rooting flow, described honestly.
//!
//! Each [`Step`] is tagged with how much of it the app can actually do:
//!
//! * [`Automation::Auto`] — the app does it for you.
//! * [`Automation::SemiAuto`] — the app does most of it; one action happens on
//!   the phone or needs a decision from you.
//! * [`Automation::Manual`] — you must do it yourself, and `manual_reason`
//!   says *why* no desktop app can do it for you.
//!
//! The honest truth for any modern Xiaomi/POCO device: the bootloader unlock
//! is gated server-side by Xiaomi (account binding, an enforced waiting period
//! of several days, a yearly per-account quota) and it wipes the phone. No PC
//! program can skip or shortcut that — doing so would require exploiting the
//! device, which this app does not do.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Automation {
    Auto,
    SemiAuto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub automation: Automation,
    /// For manual / semi-auto steps: why the app cannot fully do it.
    pub manual_reason: Option<&'static str>,
}

/// One honest sentence to show prominently in the UI.
pub const HONEST_SUMMARY: &str = "Le branchement + le flash sont automatisés. \
Mais le déverrouillage du bootloader Xiaomi (compte Mi, délai d'attente de \
plusieurs jours, 1 déblocage par compte et par an, effacement complet) est \
imposé par les serveurs Xiaomi : aucune app PC ne peut le contourner.";

/// The full, ordered rooting plan for the POCO X6 5G (`garnet`) on HyperOS,
/// using Magisk. Steps are also valid for most recent Xiaomi/POCO devices.
pub fn root_plan() -> Vec<Step> {
    vec![
        Step {
            id: "prepare_tools",
            title: "Préparer les outils",
            detail: "Télécharge et prépare adb, fastboot (Google Platform-Tools) \
                     et la dernière version de Magisk. Fait une seule fois.",
            automation: Automation::Auto,
            manual_reason: None,
        },
        Step {
            id: "enable_dev",
            title: "Activer le débogage USB + déverrouillage OEM",
            detail: "Sur le téléphone : Réglages → À propos → taper 7× sur « Version \
                     HyperOS » pour activer les Options développeur, puis activer \
                     « Débogage USB » et « Déverrouillage OEM ».",
            automation: Automation::Manual,
            manual_reason: Some("Ce sont des interrupteurs à activer sur l'écran du \
                téléphone ; un PC ne peut pas les cocher à ta place (sécurité Android)."),
        },
        Step {
            id: "detect",
            title: "Détecter le téléphone",
            detail: "Dès que tu branches le câble (débogage USB autorisé), l'app lit \
                     automatiquement le modèle, le nom de code et la version exacte du \
                     firmware.",
            automation: Automation::Auto,
            manual_reason: None,
        },
        Step {
            id: "mi_account",
            title: "Lier un compte Xiaomi",
            detail: "Sur le téléphone : Options développeur → « État du déverrouillage \
                     Mi » → ajouter ton compte Xiaomi et l'associer à l'appareil.",
            automation: Automation::Manual,
            manual_reason: Some("Exige TES identifiants Xiaomi et une validation sur le \
                téléphone. L'app ne se connecte jamais à ton compte à ta place."),
        },
        Step {
            id: "unlock_request",
            title: "Demander le déverrouillage + attendre",
            detail: "Dans l'app « Xiaomi Community », demander le déverrouillage du \
                     bootloader, puis attendre le délai imposé (souvent 3 à 7 jours, \
                     parfois plus). Limite : 1 déblocage par compte et par an.",
            automation: Automation::Manual,
            manual_reason: Some("Délai et quota appliqués par les serveurs Xiaomi. \
                Impossible à accélérer ou contourner sans exploiter l'appareil — ce que \
                cette app ne fait pas."),
        },
        Step {
            id: "bootloader_unlock",
            title: "Déverrouiller le bootloader (efface tout)",
            detail: "Avec l'outil officiel « Mi Unlock Tool » : connecter le téléphone en \
                     mode fastboot et lancer le déverrouillage. ⚠️ Cela EFFACE toutes les \
                     données du téléphone.",
            automation: Automation::Manual,
            manual_reason: Some("Nécessite l'outil officiel Xiaomi, qui signe le \
                déverrouillage avec le jeton de ton compte. Et l'effacement total doit \
                rester ta décision explicite — l'app ne wipe jamais ton téléphone seule."),
        },
        Step {
            id: "verify_unlock",
            title: "Vérifier le déverrouillage",
            detail: "L'app lit « fastboot getvar unlocked » et confirme que le bootloader \
                     est bien déverrouillé avant d'aller plus loin.",
            automation: Automation::Auto,
            manual_reason: None,
        },
        Step {
            id: "get_boot",
            title: "Récupérer le boot.img d'origine",
            detail: "Il faut le boot.img correspondant EXACTEMENT au firmware installé. \
                     L'app t'indique la version exacte à chercher et t'aide à extraire \
                     boot.img depuis la ROM fastboot (payload.bin).",
            automation: Automation::SemiAuto,
            manual_reason: Some("Télécharger la bonne ROM dépend d'un miroir externe et \
                du bon numéro de build. Un boot.img qui ne correspond pas = bootloop. \
                L'app vérifie la correspondance mais ne peut pas deviner le fichier à ta \
                place."),
        },
        Step {
            id: "patch_magisk",
            title: "Patcher le boot.img avec Magisk",
            detail: "L'app installe Magisk et pousse le boot.img sur le téléphone. Dans \
                     Magisk : Installer → « Sélectionner et patcher un fichier » → choisir \
                     le boot.img.",
            automation: Automation::SemiAuto,
            manual_reason: Some("Le patch s'exécute dans l'app Magisk sur le téléphone. \
                L'app prépare tout, mais c'est toi qui lances le patch (1 tap)."),
        },
        Step {
            id: "pull_patched",
            title: "Récupérer le boot patché",
            detail: "L'app retrouve automatiquement magisk_patched_*.img dans le dossier \
                     Download du téléphone et le rapatrie sur le PC.",
            automation: Automation::Auto,
            manual_reason: None,
        },
        Step {
            id: "flash_boot",
            title: "Flasher le boot patché",
            detail: "L'app redémarre en fastboot et lance « fastboot flash boot \
                     magisk_patched.img » puis « fastboot reboot ».",
            automation: Automation::Auto,
            manual_reason: None,
        },
        Step {
            id: "verify_root",
            title: "Vérifier le root",
            detail: "Au redémarrage, l'app vérifie que Magisk est actif. Ton POCO X6 5G \
                     est rooté 🎉",
            automation: Automation::Auto,
            manual_reason: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_has_expected_shape() {
        let plan = root_plan();
        assert_eq!(plan.first().unwrap().id, "prepare_tools");
        assert_eq!(plan.last().unwrap().id, "verify_root");

        // The Xiaomi-gated steps must be honestly marked manual with a reason.
        for id in ["enable_dev", "mi_account", "unlock_request", "bootloader_unlock"] {
            let step = plan.iter().find(|s| s.id == id).unwrap();
            assert_eq!(step.automation, Automation::Manual, "{id} must be manual");
            assert!(step.manual_reason.is_some(), "{id} must explain why");
        }

        // Flashing is fully automated.
        let flash = plan.iter().find(|s| s.id == "flash_boot").unwrap();
        assert_eq!(flash.automation, Automation::Auto);
    }

    #[test]
    fn every_non_auto_step_has_a_reason() {
        for step in root_plan() {
            match step.automation {
                Automation::Auto => assert!(step.manual_reason.is_none()),
                _ => assert!(
                    step.manual_reason.is_some(),
                    "{} should explain its limits",
                    step.id
                ),
            }
        }
    }
}
