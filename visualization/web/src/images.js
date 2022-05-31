import React from "react";

export function getLogoImage(clientDomain) {
    return (
        <img
            src={`/images/${clientDomain}/Logo.png`}
            alt={clientDomain}
            className="logo"
            onError={(ev) => {
                ev.target.src = "/images/DefaultLogo.png";
            }}
        />
    );
}
