import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

// Langue initiale : français. Ajouter une langue = ajouter un bloc `resources.<lng>`.
void i18n.use(initReactI18next).init({
  lng: 'fr',
  fallbackLng: 'fr',
  interpolation: { escapeValue: false },
  resources: {
    fr: {
      translation: {
        nav: {
          dashboard: 'Tableau de bord',
          employees: 'Employés',
          contracts: 'Contrats',
          organization: 'Organisation',
          leave: 'Congés',
          payroll: 'Paie',
          reports: 'Rapports',
          attendance: 'Pointage',
          payslips: 'Mes bulletins',
          security: 'Sécurité',
          users: 'Utilisateurs',
          audit: "Journal d'audit",
          platform: 'Sociétés',
        },
        status: {
          candidate: 'Candidat',
          active: 'Actif',
          suspended: 'Suspendu',
          on_leave: 'En congé',
          exited: 'Sorti',
          pending: 'En attente',
          approved: 'Approuvé',
          rejected: 'Refusé',
          cancelled: 'Annulé',
          draft: 'Brouillon',
          validated: 'Validée',
          closed: 'Clôturée',
          open: 'Ouverte',
          retired: 'Retiré',
        },
        role: {
          hr_admin: 'Admin RH',
          hr_manager: 'Gestionnaire RH',
          payroll_manager: 'Responsable paie',
          manager: 'Manager',
          employee: 'Employé',
          auditor: 'Auditeur',
        },
        common: {
          loading: 'Chargement…',
          save: 'Enregistrer',
          cancel: 'Annuler',
          search: 'Rechercher',
          logout: 'Se déconnecter',
          empty: 'Aucun élément',
        },
      },
    },
  },
})

export default i18n
