import React from "react";
import PropTypes from "prop-types";
import Spinner from "../../controls/Spinner";
import Item from "./Item";

const List = (props) => {
  let {
    state: { loading, roles, users },
    saveUser,
    updateUserRole,
  } = props.userList;
  return loading || !users ? (
    <Spinner />
  ) : (
    users.map((user) => (
      <Item
        key={user.id}
        user={user}
        roles={roles}
        updateRole={updateUserRole}
        saveUser={saveUser}
      />
    ))
  );
};

List.propTypes = {
  userList: PropTypes.object.isRequired,
};

export default List;
