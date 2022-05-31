import React, {Component} from "react";
import Dropdown from "../../controls/Dropdown/Dropdown";
import "../../Style.css";

class MoreLink extends Component {
    render() {
        const filteredChildren = this.props.children.filter((ch) => ch !== null);
        return !this.props.isSlidePanel ? (
            <Dropdown id="MoreDropdown" headerText="nav-link more" value="More" className="margin-right">
                {filteredChildren}
            </Dropdown>
        ) : (
            filteredChildren
        );
    }
}

export default MoreLink;
