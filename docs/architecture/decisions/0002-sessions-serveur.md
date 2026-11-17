# ADR 0002 — Authentification par sessions serveur et cookie

Jeton aléatoire de 256 bits en cookie `HttpOnly`, `SameSite=Strict`, `Secure` en production ; seul son SHA-256 est stocké. Révocation immédiate (logout, changement de mot de passe, révocation d'appareil) ; le rôle est relu à chaque requête. Aucun jeton dans `localStorage`. CSRF : `SameSite=Strict` + en-tête obligatoire `X-Requested-With: mysuite` sur toute requête modifiante. Rejeté : JWT longue durée (révocation difficile).
