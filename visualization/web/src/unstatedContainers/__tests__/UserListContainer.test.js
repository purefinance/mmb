import UserListContainer from "../UserListContainer";

describe("UserListContainer", () => {
  it("UserListContainer created", () => {
    let userList = new UserListContainer();
    expect(userList.state.loading).toEqual(false);
    expect(userList.state.users).toEqual([]);
    expect(userList.state.roles).toEqual([]);

    // !TODO user list not implemented
    // await userList.loadUserList();
  });
});
